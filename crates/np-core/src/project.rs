//! Project field rules, default states and estimate scales.

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

pub const NAME_MAX: usize = 80;
pub const DESCRIPTION_MAX: usize = 4000;
pub const IDENTIFIER_MAX: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum StateGroup {
    Backlog,
    Unstarted,
    Started,
    Completed,
    Cancelled,
}

impl StateGroup {
    pub const ALL: [StateGroup; 5] = [
        Self::Backlog,
        Self::Unstarted,
        Self::Started,
        Self::Completed,
        Self::Cancelled,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Unstarted => "unstarted",
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|g| g.as_str() == s)
            .ok_or_else(|| Error::bad_request(format!("unknown state group {s:?}")))
    }
}

/// The states every new project starts with: (name, group, color, is_default).
pub const DEFAULT_STATES: [(&str, StateGroup, &str, bool); 5] = [
    ("Backlog", StateGroup::Backlog, "#8b8d98", false),
    ("Todo", StateGroup::Unstarted, "#b9bbc6", true),
    ("In Progress", StateGroup::Started, "#e2a336", false),
    ("Done", StateGroup::Completed, "#30a46c", false),
    ("Cancelled", StateGroup::Cancelled, "#e5484d", false),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EstimateType {
    None,
    Points,
    Tshirt,
}

impl EstimateType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Points => "points",
            Self::Tshirt => "tshirt",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "points" => Self::Points,
            "tshirt" => Self::Tshirt,
            _ => Self::None,
        }
    }
    pub fn default_values(self) -> Vec<String> {
        match self {
            Self::None => vec![],
            Self::Points => ["1", "2", "3", "5", "8", "13"].map(String::from).to_vec(),
            Self::Tshirt => ["XS", "S", "M", "L", "XL", "XXL"]
                .map(String::from)
                .to_vec(),
        }
    }
}

/// Validates an estimate scale: 2–12 distinct non-empty values; points must be numbers.
pub fn validate_estimates(t: EstimateType, values: &[String]) -> Result<Vec<String>> {
    if t == EstimateType::None {
        return Ok(vec![]);
    }
    let vals: Vec<String> = values.iter().map(|v| v.trim().to_string()).collect();
    if vals.len() < 2 || vals.len() > 12 {
        return Err(Error::bad_request("an estimate scale needs 2 to 12 values"));
    }
    for (i, v) in vals.iter().enumerate() {
        if v.is_empty() || v.chars().count() > 12 {
            return Err(Error::bad_request(
                "estimate values must be 1–12 characters",
            ));
        }
        if vals[..i].contains(v) {
            return Err(Error::bad_request("estimate values must be different"));
        }
        if t == EstimateType::Points && v.parse::<f64>().map_or(true, |n| !n.is_finite() || n < 0.0)
        {
            return Err(Error::bad_request("point estimates must be numbers"));
        }
    }
    Ok(vals)
}

/// Uppercase letters and digits, starting with a letter, 1–12 characters (`WEB`, `API2`).
pub fn validate_identifier(raw: &str) -> Result<String> {
    let id = raw.trim().to_uppercase();
    let n = id.chars().count();
    let ok = (1..=IDENTIFIER_MAX).contains(&n)
        && id.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && id
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit());
    if ok {
        Ok(id)
    } else {
        Err(Error::bad_request(format!(
            "project identifier must be 1–{IDENTIFIER_MAX} letters or digits, starting with a letter"
        )))
    }
}

/// A suggestion from a name: "Mobile App" → "MA", "Website" → "WEB".
pub fn suggest_identifier(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let s: String = if words.len() >= 2 {
        words
            .iter()
            .filter_map(|w| w.chars().next())
            .take(5)
            .collect()
    } else {
        words
            .first()
            .map(|w| w.chars().take(3).collect())
            .unwrap_or_default()
    };
    let s = s.to_uppercase();
    if s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        s
    } else {
        format!("P{s}").chars().take(IDENTIFIER_MAX).collect()
    }
}

/// `#rgb` or `#rrggbb`.
pub fn validate_color(raw: &str) -> Result<String> {
    let c = raw.trim().to_lowercase();
    let hex = c.strip_prefix('#').unwrap_or("");
    if (hex.len() == 3 || hex.len() == 6) && hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(c)
    } else {
        Err(Error::bad_request("colors are #rgb or #rrggbb"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        assert_eq!(validate_identifier(" web ").unwrap(), "WEB");
        assert_eq!(validate_identifier("API2").unwrap(), "API2");
        for bad in ["", "2API", "WE B", "W-B", "ABCDEFGHIJKLM"] {
            assert!(validate_identifier(bad).is_err(), "{bad}");
        }
        assert_eq!(suggest_identifier("Mobile App"), "MA");
        assert_eq!(suggest_identifier("Website"), "WEB");
        assert_eq!(suggest_identifier("2026 plan"), "P2P");
    }

    #[test]
    fn estimates_and_colors() {
        assert!(
            validate_estimates(EstimateType::Points, &EstimateType::Points.default_values())
                .is_ok()
        );
        assert!(validate_estimates(EstimateType::Points, &["1".into(), "x".into()]).is_err());
        assert!(validate_estimates(EstimateType::Tshirt, &["S".into(), "S".into()]).is_err());
        assert!(validate_estimates(EstimateType::Tshirt, &["S".into()]).is_err());
        assert_eq!(validate_color("#ABC").unwrap(), "#abc");
        assert!(validate_color("red").is_err());
    }
}
