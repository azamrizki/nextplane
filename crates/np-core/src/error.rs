//! The error type shared by every layer. The API maps [`ErrorKind`] to an HTTP status and
//! sends `{ "error": { "code", "message" } }` (SPEC §8).

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    BadRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    MethodNotAllowed,
    Conflict,
    RateLimited,
    Internal,
}

impl ErrorKind {
    /// The stable machine-readable code sent to clients.
    pub fn code(self) -> &'static str {
        match self {
            ErrorKind::BadRequest => "bad_request",
            ErrorKind::Unauthorized => "unauthorized",
            ErrorKind::Forbidden => "forbidden",
            ErrorKind::NotFound => "not_found",
            ErrorKind::MethodNotAllowed => "method_not_allowed",
            ErrorKind::Conflict => "conflict",
            ErrorKind::RateLimited => "rate_limited",
            ErrorKind::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.code(), self.message)
    }
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::BadRequest, message)
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotFound, message)
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Forbidden, message)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Conflict, message)
    }
    /// An internal failure. The detail is logged by the caller; clients get a generic message.
    pub fn internal() -> Self {
        Self::new(ErrorKind::Internal, "internal error")
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable() {
        assert_eq!(ErrorKind::NotFound.code(), "not_found");
        assert_eq!(Error::conflict("x").to_string(), "conflict: x");
    }
}
