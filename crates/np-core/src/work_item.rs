//! Work item field rules.

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const TITLE_MAX: usize = 255;
/// Rich-text bodies (JSON and HTML each) are capped to keep rows reasonable.
pub const BODY_MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Urgent,
    High,
    Medium,
    Low,
    None,
}

impl Priority {
    pub const ALL: [Priority; 5] = [
        Self::Urgent,
        Self::High,
        Self::Medium,
        Self::Low,
        Self::None,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Urgent => "urgent",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::None => "none",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_str() == s)
            .ok_or_else(|| Error::bad_request(format!("unknown priority {s:?}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    Blocks,
    BlockedBy,
    RelatesTo,
    DuplicateOf,
}

impl RelationKind {
    /// Stored form: `blocked_by` is saved as the reverse `blocks`.
    pub fn stored(
        self,
        this: uuid::Uuid,
        other: uuid::Uuid,
    ) -> (&'static str, uuid::Uuid, uuid::Uuid) {
        match self {
            Self::Blocks => ("blocks", this, other),
            Self::BlockedBy => ("blocks", other, this),
            Self::RelatesTo => ("relates_to", this, other),
            Self::DuplicateOf => ("duplicate_of", this, other),
        }
    }
}

pub fn clean_title(raw: &str) -> Result<String> {
    let t = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        return Err(Error::bad_request("title is required"));
    }
    if t.chars().count() > TITLE_MAX {
        return Err(Error::bad_request(format!(
            "title must be at most {TITLE_MAX} characters"
        )));
    }
    Ok(t)
}

/// `YYYY-MM-DD` with a real calendar date.
pub fn validate_date(raw: &str) -> Result<String> {
    let ok = raw.len() == 10
        && raw.as_bytes()[4] == b'-'
        && raw.as_bytes()[7] == b'-'
        && raw
            .split('-')
            .all(|p| p.chars().all(|c| c.is_ascii_digit()))
        && {
            let y: i32 = raw[0..4].parse().unwrap_or(0);
            let m: u32 = raw[5..7].parse().unwrap_or(0);
            let d: u32 = raw[8..10].parse().unwrap_or(0);
            (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m) && y >= 1970
        };
    if ok {
        Ok(raw.to_string())
    } else {
        Err(Error::bad_request("dates are YYYY-MM-DD"))
    }
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

pub fn check_date_order(start: Option<&str>, due: Option<&str>) -> Result<()> {
    if let (Some(s), Some(d)) = (start, due)
        && s > d
    {
        return Err(Error::bad_request(
            "the start date must not be after the due date",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_dates_priorities() {
        assert_eq!(clean_title("  Fix   login\nbug ").unwrap(), "Fix login bug");
        assert!(clean_title("   ").is_err());
        assert!(validate_date("2026-02-29").is_err());
        assert!(validate_date("2028-02-29").is_ok());
        assert!(validate_date("2026-13-01").is_err());
        assert!(validate_date("26-01-01").is_err());
        assert!(check_date_order(Some("2026-01-02"), Some("2026-01-01")).is_err());
        assert!(check_date_order(Some("2026-01-01"), None).is_ok());
        assert_eq!(Priority::parse("urgent").unwrap(), Priority::Urgent);
        assert!(Priority::parse("p0").is_err());
    }
}
