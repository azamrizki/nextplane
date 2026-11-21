//! Export a project's work items as CSV or JSON, and import work items from CSV with a
//! preview and a column mapping (SPRINT S3.5).

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use np_core::id::Uuid;
use np_core::perm::ProjAction;
use np_core::work_item::{self, Priority};
use np_core::{Error, User};
use np_store::work_items::NewWorkItem;
use serde::{Deserialize, Serialize};

use super::projects::{ProjCtx, load_project};
use crate::AppState;
use crate::auth::CurrentUser;
use crate::error::ApiResult;
use crate::extract::{Json, Path, Query};

const MAX_ROWS: usize = 5000;
const MAX_BYTES: usize = 5 * 1024 * 1024;

#[derive(Deserialize)]
pub struct ExportQuery {
    #[serde(default)]
    format: Option<String>,
}

const CSV_HEAD: [&str; 18] = [
    "Key",
    "Title",
    "State",
    "State group",
    "Priority",
    "Assignees",
    "Labels",
    "Cycle",
    "Modules",
    "Start date",
    "Due date",
    "Estimate",
    "Parent",
    "Created by",
    "Created at",
    "Updated at",
    "Completed at",
    "Description",
];

pub async fn export(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Query(q): Query<ExportQuery>,
) -> ApiResult<Response> {
    let c = load_project(&state, &slug, pid, &me).await?;
    c.require(ProjAction::ViewWorkItems)?;
    let rows = state.store.export_items(pid).await?;
    let ident = c.row.project.identifier.clone();
    let (body, ctype, ext) = match q.format.as_deref().unwrap_or("csv") {
        "json" => (
            serde_json::to_vec_pretty(&rows).map_err(|e| {
                tracing::error!(error = %e, "export failed");
                Error::internal()
            })?,
            "application/json",
            "json",
        ),
        "csv" => {
            let mut w = csv::Writer::from_writer(Vec::new());
            let csv_err = |e: csv::Error| {
                tracing::error!(error = %e, "export failed");
                Error::internal()
            };
            w.write_record(CSV_HEAD).map_err(csv_err)?;
            for r in &rows {
                let o = |v: &Option<String>| v.clone().unwrap_or_default();
                w.write_record([
                    r.key.clone(),
                    safe_cell(&r.title),
                    r.state.clone(),
                    r.state_group.clone(),
                    r.priority.clone(),
                    r.assignees.clone(),
                    safe_cell(&r.labels),
                    safe_cell(&o(&r.cycle)),
                    safe_cell(&r.modules),
                    o(&r.start_date),
                    o(&r.due_date),
                    o(&r.estimate),
                    o(&r.parent),
                    r.created_by.clone(),
                    r.created_at.clone(),
                    r.updated_at.clone(),
                    o(&r.completed_at),
                    safe_cell(&html_to_text(&r.description_html)),
                ])
                .map_err(csv_err)?;
            }
            // A byte order mark so spreadsheet apps read UTF-8.
            let mut out = "\u{feff}".as_bytes().to_vec();
            out.extend(w.into_inner().map_err(|e| {
                tracing::error!(error = %e, "export failed");
                Error::internal()
            })?);
            (out, "text/csv; charset=utf-8", "csv")
        }
        _ => return Err(Error::bad_request("format is csv or json").into()),
    };
    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
    if let Ok(v) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{ident}-work-items.{ext}\""
    )) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

/// Readable text of stored HTML for a CSV cell: paragraphs and line breaks kept, list items
/// on their own lines, tags dropped, entities decoded.
fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else { break };
        let tag = rest[i + 1..i + j].trim().to_ascii_lowercase();
        let name = tag
            .trim_start_matches('/')
            .split([' ', '/'])
            .next()
            .unwrap_or("");
        match (tag.starts_with('/'), name) {
            (_, "br") => out.push('\n'),
            (true, "p" | "h1" | "h2" | "h3" | "h4" | "pre" | "blockquote" | "ul" | "ol") => {
                out.push_str("\n\n")
            }
            (true, "li") => out.push('\n'),
            (false, "li") => out.push_str("- "),
            _ => {}
        }
        rest = &rest[i + j + 1..];
    }
    if !rest.contains('<') {
        out.push_str(rest);
    }
    let out = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    let mut clean = String::new();
    for line in out.split('\n') {
        if line.trim().is_empty() && clean.ends_with("\n\n") {
            continue;
        }
        clean.push_str(line.trim_end());
        clean.push('\n');
    }
    clean.trim().to_string()
}

/// Text that a spreadsheet would run as a formula gets a leading apostrophe.
fn safe_cell(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{s}")
    } else {
        s.to_string()
    }
}

// ---- import ----------------------------------------------------------------------------

/// Fields a CSV column can fill.
const FIELDS: [&str; 9] = [
    "title",
    "description",
    "state",
    "priority",
    "assignees",
    "labels",
    "start_date",
    "due_date",
    "estimate",
];

fn guess(header: &str) -> Option<&'static str> {
    let h = header
        .trim()
        .trim_start_matches('\u{feff}')
        .to_lowercase()
        .replace(['_', '-'], " ");
    Some(match h.as_str() {
        "title" | "name" | "summary" | "subject" | "task" => "title",
        "description" | "body" | "details" | "notes" => "description",
        "state" | "status" => "state",
        "priority" => "priority",
        "assignee" | "assignees" | "assigned to" | "owner" => "assignees",
        "label" | "labels" | "tags" | "tag" => "labels",
        "start" | "start date" => "start_date",
        "due" | "due date" | "deadline" | "target date" => "due_date",
        "estimate" | "points" | "story points" | "size" => "estimate",
        _ => return None,
    })
}

/// Header names, and rows each with the line they start on.
type Parsed = (Vec<String>, Vec<(usize, Vec<String>)>);

/// Header and rows, each row with the line it starts on (a quoted cell may span lines).
fn parse_csv(bytes: &[u8]) -> ApiResult<Parsed> {
    if bytes.len() > MAX_BYTES {
        return Err(Error::bad_request("the file is larger than 5 MB").into());
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::bad_request("the file must be UTF-8 text"))?;
    let text = text.trim_start_matches('\u{feff}');
    // Comma, or semicolon when the first line has more of those (spreadsheets in many locales).
    let first = text.lines().next().unwrap_or("");
    let delim = if first.matches(';').count() > first.matches(',').count() {
        b';'
    } else {
        b','
    };
    let mut r = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = r
        .headers()
        .map_err(|e| Error::bad_request(format!("cannot read the CSV: {e}")))?
        .iter()
        .map(|h| h.trim().to_string())
        .collect();
    if headers.iter().all(|h| h.is_empty()) {
        return Err(Error::bad_request("the first line must name the columns").into());
    }
    let mut rows = Vec::new();
    for rec in r.records() {
        let rec = rec.map_err(|e| Error::bad_request(format!("cannot read the CSV: {e}")))?;
        if rec.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let line = rec
            .position()
            .map(|p| p.line() as usize)
            .unwrap_or(rows.len() + 2);
        rows.push((line, rec.iter().map(|c| c.to_string()).collect()));
        if rows.len() > MAX_ROWS {
            return Err(Error::bad_request(format!("at most {MAX_ROWS} rows per import")).into());
        }
    }
    Ok((headers, rows))
}

#[derive(Serialize)]
pub struct Preview {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    total_rows: usize,
    /// Field → column index.
    mapping: HashMap<&'static str, usize>,
    fields: [&'static str; 9],
}

async fn import_ctx(state: &AppState, me: &User, slug: &str, pid: Uuid) -> ApiResult<ProjCtx> {
    let c = load_project(state, slug, pid, me).await?;
    c.require_write(ProjAction::CreateWorkItem)?;
    Ok(c)
}

/// `POST /projects/{id}/import/preview` with the CSV file as the body.
pub async fn preview(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    body: Bytes,
) -> ApiResult<Json<Preview>> {
    import_ctx(&state, &me, &slug, pid).await?;
    let (headers, rows) = parse_csv(&body)?;
    let mut mapping = HashMap::new();
    for (i, h) in headers.iter().enumerate() {
        if let Some(f) = guess(h) {
            mapping.entry(f).or_insert(i);
        }
    }
    Ok(Json(Preview {
        total_rows: rows.len(),
        rows: rows.into_iter().take(10).map(|r| r.1).collect(),
        headers,
        mapping,
        fields: FIELDS,
    }))
}

#[derive(Deserialize)]
pub struct ImportReq {
    csv: String,
    mapping: HashMap<String, usize>,
}

#[derive(Serialize)]
pub struct Note {
    /// Line in the file (the header is line 1).
    line: usize,
    message: String,
}

#[derive(Serialize)]
pub struct ImportResult {
    created: usize,
    skipped: Vec<Note>,
    warnings: Vec<Note>,
}

fn split_list(s: &str) -> Vec<String> {
    s.split([';', ',', '|'])
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

pub async fn import(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, pid)): Path<(String, Uuid)>,
    Json(req): Json<ImportReq>,
) -> ApiResult<Json<ImportResult>> {
    let c = import_ctx(&state, &me, &slug, pid).await?;
    for f in req.mapping.keys() {
        if !FIELDS.contains(&f.as_str()) {
            return Err(Error::bad_request(format!("unknown field {f:?}")).into());
        }
    }
    let Some(&title_col) = req.mapping.get("title") else {
        return Err(Error::bad_request("map a column to the title").into());
    };
    let (_, rows) = parse_csv(req.csv.as_bytes())?;
    let states = state.store.states(pid).await?;
    let labels = state.store.labels(pid).await?;
    let members = state.store.project_members(pid, c.ws.workspace.id).await?;
    let estimates = &c.row.project.estimate_values;
    let col = |row: &Vec<String>, f: &str| {
        req.mapping
            .get(f)
            .and_then(|i| row.get(*i))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };

    let mut out = ImportResult {
        created: 0,
        skipped: vec![],
        warnings: vec![],
    };
    for (line, row) in &rows {
        let line = *line;
        let mut warn = |m: String| out.warnings.push(Note { line, message: m });
        let title: String = row
            .get(title_col)
            .map(|t| t.trim())
            .unwrap_or("")
            .chars()
            .filter(|c| !c.is_control())
            .take(255)
            .collect();
        if title.is_empty() {
            out.skipped.push(Note {
                line,
                message: "no title".into(),
            });
            continue;
        }
        let mut n = NewWorkItem {
            title,
            ..Default::default()
        };
        if let Some(d) = col(row, "description") {
            n.description_html = crate::mcp::text_to_html(&d);
        }
        if let Some(s) = col(row, "state") {
            let s_l = s.to_lowercase();
            match states
                .iter()
                .find(|x| x.name.to_lowercase() == s_l)
                .or_else(|| {
                    states
                        .iter()
                        .find(|x| x.group.0.as_str() == s_l && x.is_default)
                })
                .or_else(|| states.iter().find(|x| x.group.0.as_str() == s_l))
            {
                Some(st) => n.state_id = Some(st.id),
                None => warn(format!("unknown state {s:?}, used the default")),
            }
        }
        if let Some(p) = col(row, "priority") {
            match Priority::parse(&p.to_lowercase()) {
                Ok(p) => n.priority = Some(p),
                Err(_) => warn(format!("unknown priority {p:?}")),
            }
        }
        if let Some(a) = col(row, "assignees") {
            let mut ids = Vec::new();
            for who in split_list(&a) {
                let w = who.to_lowercase();
                match members
                    .iter()
                    .find(|m| m.email.to_lowercase() == w || m.name.to_lowercase() == w)
                {
                    Some(m) => ids.push(m.user_id),
                    None => warn(format!("{who:?} is not a member of the project")),
                }
            }
            n.assignee_ids = Some(ids);
        }
        if let Some(l) = col(row, "labels") {
            for name in split_list(&l) {
                let nl = name.to_lowercase();
                match labels.iter().find(|x| x.name.to_lowercase() == nl) {
                    Some(x) => n.label_ids.push(x.id),
                    None => warn(format!("unknown label {name:?}")),
                }
            }
        }
        for (f, slot) in [
            ("start_date", &mut n.start_date),
            ("due_date", &mut n.due_date),
        ] {
            if let Some(d) = col(row, f) {
                match work_item::validate_date(d.get(..10).unwrap_or(&d)) {
                    Ok(v) => *slot = Some(v),
                    Err(_) => warn(format!("{f} {d:?} is not YYYY-MM-DD")),
                }
            }
        }
        if let (Some(s), Some(d)) = (&n.start_date, &n.due_date)
            && s > d
        {
            warn("start date after due date; dates left out".into());
            n.start_date = None;
            n.due_date = None;
        }
        if let Some(e) = col(row, "estimate") {
            if estimates.iter().any(|v| v.eq_ignore_ascii_case(&e)) {
                n.estimate = estimates
                    .iter()
                    .find(|v| v.eq_ignore_ascii_case(&e))
                    .cloned();
            } else {
                warn(format!("estimate {e:?} is not one of the project's values"));
            }
        }
        match state.store.create_work_item(pid, &n, me.id).await {
            Ok(_) => out.created += 1,
            Err(e) => out.skipped.push(Note {
                line,
                message: e.message.clone(),
            }),
        }
    }
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_text_keeps_structure() {
        assert_eq!(
            html_to_text(
                "<p>Steps:<br>1. open</p><ul><li><p>a &amp; b</p></li><li>c</li></ul><p>x &lt;y&gt;</p>"
            ),
            "Steps:\n1. open\n\n- a & b\n\n- c\n\nx <y>"
        );
        assert_eq!(safe_cell("=SUM(A1)"), "'=SUM(A1)");
        assert_eq!(safe_cell("-1"), "'-1");
        assert_eq!(guess("Story Points"), Some("estimate"));
    }
}

// ---- Workspace exports (S3.7, as in Plane's workspace settings › Exports) -------------------

use np_core::perm::{self as perm_rules, ProjRole, WsRole};
use np_store::projects::Project;
use np_store::transfer::{ExportEntry, ExportRow};

/// Live projects where you may export: those you are a member of as contributor or admin.
async fn exportable(
    state: &AppState,
    slug: &str,
    me: &User,
) -> ApiResult<(np_store::workspaces::MyWorkspace, Vec<Project>)> {
    let ws = super::workspaces::load(state, slug, me).await?;
    if ws.role == WsRole::Guest {
        return Err(Error::forbidden("guests cannot export work items").into());
    }
    let list = state
        .store
        .workspace_projects(ws.workspace.id, me.id, false)
        .await?
        .into_iter()
        .filter(|r| {
            r.member_role.0.is_some()
                && perm_rules::effective_proj_role(ws.role, r.member_role.0)
                    .is_some_and(|role| role >= ProjRole::Contributor)
        })
        .map(|r| r.project)
        .collect();
    Ok((ws, list))
}

fn row_cells(r: &ExportRow) -> Vec<String> {
    let o = |v: &Option<String>| v.clone().unwrap_or_default();
    vec![
        r.key.clone(),
        safe_cell(&r.title),
        r.state.clone(),
        r.state_group.clone(),
        r.priority.clone(),
        r.assignees.clone(),
        safe_cell(&r.labels),
        safe_cell(&o(&r.cycle)),
        safe_cell(&r.modules),
        o(&r.start_date),
        o(&r.due_date),
        o(&r.estimate),
        o(&r.parent),
        r.created_by.clone(),
        r.created_at.clone(),
        r.updated_at.clone(),
        o(&r.completed_at),
        safe_cell(&html_to_text(&r.description_html)),
        o(&r.archived_at),
    ]
}

/// One file for all the projects: CSV and JSON rows carry their project; XLSX has a sheet per
/// project. Archived items are included; drafts and intake requests are not.
async fn build_export(
    state: &AppState,
    projects: &[Project],
    format: &str,
) -> ApiResult<(Vec<u8>, &'static str, &'static str, i64)> {
    let mut all: Vec<(&Project, Vec<ExportRow>)> = Vec::new();
    for p in projects {
        all.push((p, state.store.export_items(p.id).await?));
    }
    let count = all.iter().map(|(_, r)| r.len() as i64).sum();
    let fail = |e: String| {
        tracing::error!(error = %e, "export failed");
        Error::internal()
    };
    let mut head: Vec<&str> = CSV_HEAD.to_vec();
    head.push("Archived at");
    Ok(match format {
        "csv" => {
            let mut w = csv::Writer::from_writer(Vec::new());
            let mut h = vec!["Project", "Project key"];
            h.extend(&head);
            w.write_record(&h).map_err(|e| fail(e.to_string()))?;
            for (p, rows) in &all {
                for r in rows {
                    let mut cells = vec![safe_cell(&p.name), p.identifier.clone()];
                    cells.extend(row_cells(r));
                    w.write_record(&cells).map_err(|e| fail(e.to_string()))?;
                }
            }
            let mut out = "\u{feff}".as_bytes().to_vec();
            out.extend(w.into_inner().map_err(|e| fail(e.to_string()))?);
            (out, "text/csv; charset=utf-8", "csv", count)
        }
        "json" => {
            let list: Vec<serde_json::Value> = all
                .iter()
                .flat_map(|(p, rows)| {
                    rows.iter().map(|r| {
                        let mut v = serde_json::to_value(r).unwrap_or_default();
                        v["project"] = serde_json::json!(p.name);
                        v["project_identifier"] = serde_json::json!(p.identifier);
                        v
                    })
                })
                .collect();
            (
                serde_json::to_vec_pretty(&list).map_err(|e| fail(e.to_string()))?,
                "application/json",
                "json",
                count,
            )
        }
        "xlsx" => {
            let names = crate::xlsx::sheet_names(
                &all.iter().map(|(p, _)| p.name.clone()).collect::<Vec<_>>(),
            );
            let sheets: Vec<crate::xlsx::Sheet> = all
                .iter()
                .zip(names)
                .map(|((_, rows), name)| {
                    let mut grid: Vec<Vec<crate::xlsx::Cell>> =
                        vec![head.iter().map(|h| (*h).into()).collect()];
                    for r in rows {
                        let mut cells: Vec<crate::xlsx::Cell> =
                            row_cells(r).into_iter().map(Into::into).collect();
                        // Estimates that are numbers stay numbers.
                        if let Some(n) = r.estimate.as_deref().and_then(|e| e.parse::<f64>().ok()) {
                            cells[11] = crate::xlsx::Cell::Number(n);
                        }
                        grid.push(cells);
                    }
                    crate::xlsx::Sheet { name, rows: grid }
                })
                .collect();
            (
                crate::xlsx::workbook(&sheets),
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                "xlsx",
                count,
            )
        }
        _ => return Err(Error::bad_request("format is csv, json or xlsx").into()),
    })
}

#[derive(Deserialize)]
pub struct WsExportReq {
    format: String,
    /// Empty: every project you may export.
    #[serde(default)]
    project_ids: Vec<Uuid>,
}

/// `POST /workspaces/{slug}/exports`: records the export and answers with it; the file comes
/// from its `download` link.
pub async fn ws_export(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Json(req): Json<WsExportReq>,
) -> ApiResult<(StatusCode, Json<ExportEntry>)> {
    let (ws, mine) = exportable(&state, &slug, &me).await?;
    if !matches!(req.format.as_str(), "csv" | "json" | "xlsx") {
        return Err(Error::bad_request("format is csv, json or xlsx").into());
    }
    let mut ids = req.project_ids.clone();
    ids.sort();
    ids.dedup();
    if let Some(bad) = ids.iter().find(|id| !mine.iter().any(|p| p.id == **id)) {
        return Err(Error::bad_request(format!("you cannot export project {bad}")).into());
    }
    let chosen: Vec<Project> = mine
        .into_iter()
        .filter(|p| ids.is_empty() || ids.contains(&p.id))
        .collect();
    if chosen.is_empty() {
        return Err(Error::bad_request("there is no project to export").into());
    }
    let mut items = 0;
    for p in &chosen {
        items += state.store.export_items(p.id).await?.len() as i64;
    }
    let e = state
        .store
        .add_export(ws.workspace.id, me.id, &req.format, &ids, items)
        .await?;
    Ok((StatusCode::CREATED, Json(e)))
}

#[derive(Deserialize)]
pub struct ExportsQuery {
    page: Option<i64>,
}

/// Your previous exports, newest first, ten a page.
pub async fn ws_exports(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path(slug): Path<String>,
    Query(q): Query<ExportsQuery>,
) -> ApiResult<Json<Vec<ExportEntry>>> {
    let (ws, _) = exportable(&state, &slug, &me).await?;
    let page = q.page.unwrap_or(1).max(1);
    Ok(Json(
        state
            .store
            .exports(ws.workspace.id, me.id, 10, (page - 1) * 10)
            .await?,
    ))
}

/// The file of an export, made now from the projects it names that you may still export.
pub async fn ws_export_download(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
    Path((slug, id)): Path<(String, Uuid)>,
) -> ApiResult<Response> {
    let (ws, mine) = exportable(&state, &slug, &me).await?;
    let e = state
        .store
        .export_entry(ws.workspace.id, me.id, id)
        .await?
        .ok_or_else(|| Error::not_found("export not found"))?;
    let chosen: Vec<Project> = mine
        .into_iter()
        .filter(|p| e.project_ids.is_empty() || e.project_ids.contains(&p.id))
        .collect();
    let (body, ctype, ext, _) = build_export(&state, &chosen, &e.format).await?;
    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(ctype));
    let day = e.created_at.get(..10).unwrap_or("export");
    if let Ok(v) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}-work-items-{day}.{ext}\"",
        ws.workspace.slug
    )) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}
