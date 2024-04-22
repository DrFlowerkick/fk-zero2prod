//! src/authentication/mod.rs

mod middleware;
mod password;

pub use middleware::{reject_anonymous_users, UserId};
pub use password::{change_password_in_db, validate_credentials, AuthError, Credentials};
