//! Users and the validation rules for their fields.

use serde::Serialize;
use uuid::Uuid;

use crate::{Error, Result};

pub const PASSWORD_MIN: usize = 8;
pub const PASSWORD_MAX: usize = 128;
pub const NAME_MAX: usize = 80;
pub const INSTANCE_NAME_MAX: usize = 80;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub avatar: Option<String>,
    pub is_instance_admin: bool,
    pub is_bot: bool,
    pub created_at: String,
}

/// Trims and lowercases an email and checks its basic shape (`local@domain.tld`).
pub fn normalize_email(raw: &str) -> Result<String> {
    let email = raw.trim().to_lowercase();
    let ok = email.len() <= 254
        && !email.chars().any(char::is_whitespace)
        && match email.split_once('@') {
            Some((local, domain)) => {
                !local.is_empty()
                    && !domain.contains('@')
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
            }
            None => false,
        };
    if ok {
        Ok(email)
    } else {
        Err(Error::bad_request("enter a valid email address"))
    }
}

pub fn validate_password(pw: &str) -> Result<()> {
    let n = pw.chars().count();
    if n < PASSWORD_MIN {
        return Err(Error::bad_request(format!(
            "password must be at least {PASSWORD_MIN} characters"
        )));
    }
    if n > PASSWORD_MAX {
        return Err(Error::bad_request(format!(
            "password must be at most {PASSWORD_MAX} characters"
        )));
    }
    Ok(())
}

/// Trims a display name; empty is allowed (the UI then shows the email).
pub fn normalize_name(raw: &str, max: usize, field: &str) -> Result<String> {
    let name = raw.trim().to_string();
    if name.chars().count() > max {
        return Err(Error::bad_request(format!(
            "{field} must be at most {max} characters"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(Error::bad_request(format!(
            "{field} contains invalid characters"
        )));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails() {
        assert_eq!(
            normalize_email("  Ana@Example.COM ").unwrap(),
            "ana@example.com"
        );
        for bad in [
            "", "ana", "ana@", "@x.io", "a@b", "a b@x.io", "a@x.io.", "a@@x.io",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn passwords() {
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password(&"x".repeat(129)).is_err());
    }

    #[test]
    fn names() {
        assert_eq!(normalize_name("  Ana ", NAME_MAX, "name").unwrap(), "Ana");
        assert!(normalize_name(&"x".repeat(81), NAME_MAX, "name").is_err());
        assert!(normalize_name("a\u{7}b", NAME_MAX, "name").is_err());
    }
}
