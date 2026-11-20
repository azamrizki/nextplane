//! NextPlane domain layer: types, validation and permissions. No I/O lives here.

pub mod cycle;
pub mod error;
pub mod id;
pub mod perm;
pub mod project;
pub mod user;
pub mod work_item;
pub mod workspace;

pub use error::{Error, ErrorKind, Result};
pub use user::User;
