//! src/routes/subscriptions/mod.rs

mod confirm;
mod get;
mod post;
mod token;

pub use confirm::*;
pub use get::subscription_form;
pub use post::*;
pub use token::*;
