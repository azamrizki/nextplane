//! Workspace field rules.

use crate::{Error, Result};

pub const NAME_MAX: usize = 80;
pub const SLUG_MIN: usize = 3;
pub const SLUG_MAX: usize = 48;

/// Paths the web app or API use at the top level; a workspace cannot take them as its URL.
const RESERVED: &[&str] = &[
    "api",
    "assets",
    "admin",
    "login",
    "logout",
    "signup",
    "setup",
    "invite",
    "invites",
    "reset",
    "forgot",
    "profile",
    "settings",
    "create-workspace",
    "workspaces",
    "onboarding",
    "help",
    "static",
    "public",
    "health",
    "new",
    "me",
    "god-mode",
    "icon.svg",
    "favicon.ico",
];

/// Lowercase letters, digits and single hyphens, not at the ends.
pub fn validate_slug(raw: &str) -> Result<String> {
    let slug = raw.trim().to_lowercase();
    let n = slug.chars().count();
    if !(SLUG_MIN..=SLUG_MAX).contains(&n) {
        return Err(Error::bad_request(format!(
            "workspace URL must be {SLUG_MIN}–{SLUG_MAX} characters"
        )));
    }
    let ok_chars = slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !ok_chars || slug.starts_with('-') || slug.ends_with('-') || slug.contains("--") {
        return Err(Error::bad_request(
            "workspace URL may use lowercase letters, digits and single hyphens",
        ));
    }
    if RESERVED.contains(&slug.as_str()) {
        return Err(Error::bad_request("this workspace URL is reserved"));
    }
    Ok(slug)
}

/// A slug suggestion from a name: "Acme Corp!" → "acme-corp".
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-')
        .chars()
        .take(SLUG_MAX)
        .collect::<String>()
        .trim_end_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(validate_slug(" Acme-2 ").unwrap(), "acme-2");
        for bad in [
            "ab",
            "-acme",
            "acme-",
            "ac--me",
            "ac me",
            "acmé",
            "api",
            "login",
            &"a".repeat(49),
        ] {
            assert!(validate_slug(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn slugify_names() {
        assert_eq!(slugify("Acme Corp!"), "acme-corp");
        assert_eq!(slugify("  --Tim  Dev 2026-- "), "tim-dev-2026");
        assert_eq!(
            slugify("Kopi Kenangan Sejahtera"),
            "kopi-kenangan-sejahtera"
        );
    }
}
