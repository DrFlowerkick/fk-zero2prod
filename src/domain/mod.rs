//! src/domain/mod.rs

mod new_subscriber;
mod subscriber_email;
mod subscriber_name;
mod subscriber_token;

pub use new_subscriber::NewSubscriber;
pub use new_subscriber::NewSubscriberError;
pub use subscriber_email::SubscriberEmail;
pub use subscriber_name::SubscriberName;
pub use subscriber_token::SubscriberToken;
