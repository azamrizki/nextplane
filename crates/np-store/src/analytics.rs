//! Analytics (SPRINT S3.4): counts and estimate points of live work items in a set of
//! projects, broken down by one dimension and optionally segmented by a second, plus the
//! numbers for the overview tiles and a weekly created/completed trend.

use np_core::id::Uuid;
use np_core::{Error, Result};
use serde::Serialize;
use time::{Duration, OffsetDateTime};

use crate::{Store, db_err, fmt_ts, parse_ts};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dim {
    StateGroup,
    State,
    Priority,
    Assignee,
    Label,
    Cycle,
    Module,
    Project,
}

impl Dim {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "state_group" => Self::StateGroup,
            "state" => Self::State,
            "priority" => Self::Priority,
            "assignee" => Self::Assignee,
            "label" => Self::Label,
            "cycle" => Self::Cycle,
            "module" => Self::Module,
            "project" => Self::Project,
            _ => return None,
        })
    }

    /// (key, label, color, natural order or NULL, joins) with table aliases ending in `sfx`.
    fn sql(self, sfx: &str) -> (String, String, String, String, String) {
        let (k, l, c, o, j) = match self {
            Self::StateGroup => (
                "s.grp".into(),
                "s.grp".into(),
                "NULL".into(),
                "CASE s.grp WHEN 'backlog' THEN 0 WHEN 'unstarted' THEN 1 WHEN 'started' THEN 2 WHEN 'completed' THEN 3 ELSE 4 END".into(),
                String::new(),
            ),
            Self::State => ("s.name".into(), "s.name".into(), "s.color".into(), "s.sort_order".into(), String::new()),
            Self::Priority => (
                "w.priority".into(),
                "w.priority".into(),
                "NULL".into(),
                "CASE w.priority WHEN 'urgent' THEN 0 WHEN 'high' THEN 1 WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END".into(),
                String::new(),
            ),
            Self::Assignee => (
                format!("hex(a{sfx}.user_id)"),
                format!("COALESCE(NULLIF(u{sfx}.name, ''), u{sfx}.email)"),
                "NULL".into(),
                "NULL".into(),
                format!("LEFT JOIN work_item_assignees a{sfx} ON a{sfx}.work_item_id = w.id LEFT JOIN users u{sfx} ON u{sfx}.id = a{sfx}.user_id"),
            ),
            Self::Label => (
                format!("l{sfx}.name"),
                format!("l{sfx}.name"),
                format!("l{sfx}.color"),
                "NULL".into(),
                format!("LEFT JOIN work_item_labels wl{sfx} ON wl{sfx}.work_item_id = w.id LEFT JOIN labels l{sfx} ON l{sfx}.id = wl{sfx}.label_id"),
            ),
            Self::Cycle => (
                format!("hex(c{sfx}.id)"),
                format!("c{sfx}.name"),
                "NULL".into(),
                format!("julianday(c{sfx}.start_date)"),
                format!("LEFT JOIN cycles c{sfx} ON c{sfx}.id = w.cycle_id AND c{sfx}.deleted_at IS NULL"),
            ),
            Self::Module => (
                format!("hex(m{sfx}.id)"),
                format!("m{sfx}.name"),
                "NULL".into(),
                "NULL".into(),
                format!(
                    "LEFT JOIN work_item_modules wm{sfx} ON wm{sfx}.work_item_id = w.id LEFT JOIN modules m{sfx} ON m{sfx}.id = wm{sfx}.module_id AND m{sfx}.deleted_at IS NULL"
                ),
            ),
            Self::Project => ("hex(p.id)".into(), "p.name".into(), "NULL".into(), "NULL".into(), String::new()),
        };
        (k, l, c, o, j)
    }

    fn natural(self) -> bool {
        matches!(
            self,
            Self::StateGroup | Self::State | Self::Priority | Self::Cycle
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    Count,
    Points,
}

#[derive(Debug, Clone, Serialize)]
pub struct Bucket {
    /// `""` for "none" (no assignee, no label…), `"__other"` for the folded tail.
    pub key: String,
    pub label: String,
    pub color: Option<String>,
    pub total: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Cell {
    pub x: String,
    pub segment: Option<String>,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Breakdown {
    pub x: Vec<Bucket>,
    pub segments: Vec<Bucket>,
    pub cells: Vec<Cell>,
    pub total: f64,
}

pub const OTHER: &str = "__other";
const MAX_X: usize = 30;
const MAX_SEGMENTS: usize = 7;

#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub total: i64,
    pub open: i64,
    pub completed_30d: i64,
    pub overdue: i64,
    pub points_total: f64,
    pub points_open: f64,
    pub weeks: Vec<Week>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Week {
    /// Monday (UTC) the week starts.
    pub start: String,
    pub created: i64,
    pub completed: i64,
}

fn in_list(qb: &mut sqlx::QueryBuilder<'_, sqlx::Sqlite>, projects: &[Uuid]) {
    qb.push("(");
    let mut sep = qb.separated(", ");
    for p in projects {
        sep.push_bind(*p);
    }
    qb.push(")");
}

const LIVE: &str = "w.deleted_at IS NULL AND w.is_draft = 0 AND w.archived_at IS NULL";
const POINTS: &str = "CASE WHEN p.estimate_type = 'points' THEN COALESCE(CAST(w.estimate AS REAL), 0.0) ELSE 0.0 END";

type Row = (
    String,
    String,
    Option<String>,
    Option<f64>,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    f64,
    Option<f64>,
);

impl Store {
    pub async fn breakdown(
        &self,
        projects: &[Uuid],
        x: Dim,
        segment: Option<Dim>,
        measure: Measure,
    ) -> Result<Breakdown> {
        if segment == Some(x) {
            return Err(Error::bad_request("segment by a different dimension"));
        }
        if projects.is_empty() {
            return Ok(Breakdown {
                x: vec![],
                segments: vec![],
                cells: vec![],
                total: 0.0,
            });
        }
        let (xk, xl, xc, xo, xj) = x.sql("_x");
        let (sk, sl, sc, so, sj) = match segment {
            Some(s) => s.sql("_s"),
            None => (
                "NULL".into(),
                "NULL".into(),
                "NULL".into(),
                "NULL".into(),
                String::new(),
            ),
        };
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(format!(
            "SELECT COALESCE(xk, ''), COALESCE(MAX(xl), ''), MAX(xc), CAST(MIN(xo) AS REAL), sk, MAX(sl), MAX(sc), COUNT(*), CAST(COALESCE(SUM(pts), 0) AS REAL), CAST(MIN(so) AS REAL) FROM (
               SELECT DISTINCT w.id, {xk} AS xk, {xl} AS xl, {xc} AS xc, {xo} AS xo, {sk} AS sk, {sl} AS sl, {sc} AS sc, {so} AS so, {POINTS} AS pts
               FROM work_items w JOIN projects p ON p.id = w.project_id JOIN states s ON s.id = w.state_id {xj} {sj}
               WHERE {LIVE} AND w.project_id IN "
        ));
        in_list(&mut qb, projects);
        qb.push(") GROUP BY xk, sk");
        let rows: Vec<Row> = qb
            .build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        let val = |count: i64, pts: f64| {
            if measure == Measure::Count {
                count as f64
            } else {
                pts
            }
        };

        // x buckets with totals and order.
        let mut xs: Vec<(Bucket, Option<f64>)> = Vec::new();
        let mut segs: Vec<(Bucket, Option<f64>)> = Vec::new();
        for (k, l, c, o, s, slab, scol, count, pts, so) in &rows {
            let v = val(*count, *pts);
            match xs.iter_mut().find(|(b, _)| &b.key == k) {
                Some((b, _)) => b.total += v,
                None => xs.push((
                    Bucket {
                        key: k.clone(),
                        label: l.clone(),
                        color: c.clone(),
                        total: v,
                    },
                    *o,
                )),
            }
            if segment.is_some() {
                let sk = s.clone().unwrap_or_default();
                match segs.iter_mut().find(|(b, _)| b.key == sk) {
                    Some((b, _)) => b.total += v,
                    None => segs.push((
                        Bucket {
                            key: sk,
                            label: slab.clone().unwrap_or_default(),
                            color: scol.clone(),
                            total: v,
                        },
                        *so,
                    )),
                }
            }
        }
        // Natural order where the dimension has one, else largest first; "none" last.
        if x.natural() {
            xs.sort_by(|a, b| {
                (a.0.key.is_empty(), a.1.unwrap_or(f64::MAX))
                    .partial_cmp(&(b.0.key.is_empty(), b.1.unwrap_or(f64::MAX)))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            xs.sort_by(|a, b| {
                (a.0.key.is_empty(), -a.0.total)
                    .partial_cmp(&(b.0.key.is_empty(), -b.0.total))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        if segment.is_some_and(Dim::natural) {
            segs.sort_by(|a, b| {
                (a.0.key.is_empty(), a.1.unwrap_or(f64::MAX))
                    .partial_cmp(&(b.0.key.is_empty(), b.1.unwrap_or(f64::MAX)))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            segs.sort_by(|a, b| {
                (a.0.key.is_empty(), -a.0.total)
                    .partial_cmp(&(b.0.key.is_empty(), -b.0.total))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        let mut segs: Vec<Bucket> = segs.into_iter().map(|(b, _)| b).collect();
        let mut xs: Vec<Bucket> = xs.into_iter().map(|(b, _)| b).collect();
        let fold_x: Vec<String> = if xs.len() > MAX_X {
            xs.split_off(MAX_X - 1).into_iter().map(|b| b.key).collect()
        } else {
            vec![]
        };
        let fold_s: Vec<String> = if segs.len() > MAX_SEGMENTS + 1 {
            segs.split_off(MAX_SEGMENTS)
                .into_iter()
                .map(|b| b.key)
                .collect()
        } else {
            vec![]
        };

        let mut cells: Vec<Cell> = Vec::new();
        for (k, _, _, _, s, _, _, count, pts, _) in &rows {
            let xk = if fold_x.contains(k) {
                OTHER.to_string()
            } else {
                k.clone()
            };
            let sk = segment.map(|_| {
                let s = s.clone().unwrap_or_default();
                if fold_s.contains(&s) {
                    OTHER.to_string()
                } else {
                    s
                }
            });
            let v = val(*count, *pts);
            match cells.iter_mut().find(|c| c.x == xk && c.segment == sk) {
                Some(c) => c.value += v,
                None => cells.push(Cell {
                    x: xk,
                    segment: sk,
                    value: v,
                }),
            }
        }
        if !fold_x.is_empty() {
            let total = cells.iter().filter(|c| c.x == OTHER).map(|c| c.value).sum();
            xs.push(Bucket {
                key: OTHER.into(),
                label: String::new(),
                color: None,
                total,
            });
        }
        if !fold_s.is_empty() {
            let total = cells
                .iter()
                .filter(|c| c.segment.as_deref() == Some(OTHER))
                .map(|c| c.value)
                .sum();
            segs.push(Bucket {
                key: OTHER.into(),
                label: String::new(),
                color: None,
                total,
            });
        }
        // Distinct items overall (an item with two labels counts once in the total).
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(format!(
            "SELECT COUNT(*), CAST(COALESCE(SUM({POINTS}), 0) AS REAL) FROM work_items w JOIN projects p ON p.id = w.project_id WHERE {LIVE} AND w.project_id IN "
        ));
        in_list(&mut qb, projects);
        let (n, pts): (i64, f64) = qb
            .build_query_as()
            .fetch_one(self.pool())
            .await
            .map_err(db_err)?;
        Ok(Breakdown {
            x: xs,
            segments: segs,
            cells,
            total: val(n, pts),
        })
    }

    pub async fn overview(
        &self,
        projects: &[Uuid],
        weeks: usize,
        now: OffsetDateTime,
    ) -> Result<Overview> {
        let today = now.date();
        let monday = today - Duration::days(today.weekday().number_days_from_monday() as i64);
        let first = monday - Duration::weeks(weeks as i64 - 1);
        let mut out = Overview {
            total: 0,
            open: 0,
            completed_30d: 0,
            overdue: 0,
            points_total: 0.0,
            points_open: 0.0,
            weeks: (0..weeks)
                .map(|i| Week {
                    start: (first + Duration::weeks(i as i64)).to_string(),
                    created: 0,
                    completed: 0,
                })
                .collect(),
        };
        if projects.is_empty() {
            return Ok(out);
        }
        let since_30 = fmt_ts(now - Duration::days(30));
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT COUNT(*),
                    COALESCE(SUM(s.grp NOT IN ('completed', 'cancelled')), 0),
                    COALESCE(SUM(w.completed_at >= "
                .to_string(),
        );
        qb.push_bind(&since_30);
        qb.push(" AND s.grp = 'completed'), 0), COALESCE(SUM(s.grp NOT IN ('completed', 'cancelled') AND w.due_date < ");
        qb.push_bind(today.to_string());
        qb.push(format!(
            "), 0), CAST(COALESCE(SUM({POINTS}), 0) AS REAL), CAST(COALESCE(SUM(CASE WHEN s.grp NOT IN ('completed', 'cancelled') THEN {POINTS} ELSE 0.0 END), 0) AS REAL)
             FROM work_items w JOIN projects p ON p.id = w.project_id JOIN states s ON s.id = w.state_id WHERE {LIVE} AND w.project_id IN "
        ));
        in_list(&mut qb, projects);
        let (t, o, c, d, pt, po): (i64, i64, i64, i64, f64, f64) = qb
            .build_query_as()
            .fetch_one(self.pool())
            .await
            .map_err(db_err)?;
        (
            out.total,
            out.open,
            out.completed_30d,
            out.overdue,
            out.points_total,
            out.points_open,
        ) = (t, o, c, d, pt, po);

        // Weekly trend (archived items still count: they were created and completed).
        let start = fmt_ts(first.midnight().assume_utc());
        let mut qb = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT w.created_at, w.completed_at FROM work_items w JOIN states s ON s.id = w.state_id WHERE w.deleted_at IS NULL AND w.is_draft = 0 AND (w.created_at >= ",
        );
        qb.push_bind(&start);
        qb.push(" OR (w.completed_at >= ");
        qb.push_bind(&start);
        qb.push(" AND s.grp = 'completed')) AND w.project_id IN ");
        in_list(&mut qb, projects);
        let rows: Vec<(String, Option<String>)> = qb
            .build_query_as()
            .fetch_all(self.pool())
            .await
            .map_err(db_err)?;
        let week_of = |ts: &str| -> Option<usize> {
            let d = parse_ts(ts)?.date();
            let days = (d - first).whole_days();
            (days >= 0)
                .then_some((days / 7) as usize)
                .filter(|i| *i < weeks)
        };
        for (created, completed) in rows {
            if let Some(i) = week_of(&created) {
                out.weeks[i].created += 1;
            }
            if let Some(i) = completed.as_deref().and_then(week_of) {
                out.weeks[i].completed += 1;
            }
        }
        Ok(out)
    }
}
