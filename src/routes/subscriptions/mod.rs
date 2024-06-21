//! src/routes/subscriptions/mod.rs

mod get;
mod post;
mod token;

pub use get::subscription_form;
pub use post::*;
pub use token::*;