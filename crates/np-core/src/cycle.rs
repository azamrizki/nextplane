//! Cycle rules: status from dates, overlap.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CycleStatus {
    Draft,
    Upcoming,
    Current,
    Completed,
}

/// Status on `today` (`YYYY-MM-DD`); a cycle without both dates is a draft.
pub fn status(start: Option<&str>, end: Option<&str>, today: &str) -> CycleStatus {
    match (start, end) {
        (Some(s), Some(_)) if today < s => CycleStatus::Upcoming,
        (Some(_), Some(e)) if today > e => CycleStatus::Completed,
        (Some(_), Some(_)) => CycleStatus::Current,
        _ => CycleStatus::Draft,
    }
}

/// Whether two inclusive date ranges share a day.
pub fn overlaps(a: (&str, &str), b: (&str, &str)) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}

/// Days from `start` to `end` inclusive, as `YYYY-MM-DD` (at most 366).
pub fn days(start: &str, end: &str) -> Vec<String> {
    let parse = |d: &str| -> Option<time::Date> {
        let f = time::macros::format_description!("[year]-[month]-[day]");
        time::Date::parse(d, f).ok()
    };
    let (Some(mut d), Some(e)) = (parse(start), parse(end)) else {
        return vec![];
    };
    let mut out = Vec::new();
    while d <= e && out.len() < 366 {
        out.push(format!(
            "{:04}-{:02}-{:02}",
            d.year(),
            d.month() as u8,
            d.day()
        ));
        match d.next_day() {
            Some(n) => d = n,
            None => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_overlap_and_days() {
        assert_eq!(
            status(None, Some("2026-01-01"), "2026-01-01"),
            CycleStatus::Draft
        );
        assert_eq!(
            status(Some("2026-11-21"), Some("2026-12-01"), "2026-11-20"),
            CycleStatus::Upcoming
        );
        assert_eq!(
            status(Some("2026-11-20"), Some("2026-12-01"), "2026-11-20"),
            CycleStatus::Current
        );
        assert_eq!(
            status(Some("2026-11-01"), Some("2026-11-19"), "2026-11-20"),
            CycleStatus::Completed
        );
        assert!(overlaps(
            ("2026-01-01", "2026-01-14"),
            ("2026-01-14", "2026-01-28")
        ));
        assert!(!overlaps(
            ("2026-01-01", "2026-01-13"),
            ("2026-01-14", "2026-01-28")
        ));
        assert_eq!(
            days("2026-02-27", "2026-03-02"),
            ["2026-02-27", "2026-02-28", "2026-03-01", "2026-03-02"]
        );
        assert_eq!(days("2026-01-01", "2026-01-01"), ["2026-01-01"]);
    }
}
