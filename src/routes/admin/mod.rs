//! src/routes/admin/mod.rs

mod dashboard;
mod delivery_overview;
mod logout;
mod newsletters;
mod password;

pub use dashboard::admin_dashboard;
pub use delivery_overview::*;
pub use logout::log_out;
pub use newsletters::*;
pub use password::*;
