//! src/domain/mod.rs

mod new_subscriber;
mod subscriber_email;
mod subscriber_name;
mod subscriber_token;

pub use new_subscriber::NewSubscriber;
pub use subscriber_email::SubscriberEmail;
pub use subscriber_name::SubscriberName;
pub use subscriber_token::SubscriberToken;

/// Validation error for domain data
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("`{0}` is not a valid subscriber email.")]
    InvalidEmail(String),
    #[error("`{0}` is not a valid subscriber name.")]
    InvalidName(String),
    #[error("`{0}` is not a valid subscriber token.")]
    InvalidToken(String),
}
