//! src/domain/new_subscriber.rs

use crate::domain::SubscriberEmail;
use crate::domain::SubscriberName;

pub struct NewSubscriber {
    pub email: SubscriberEmail,
    pub name: SubscriberName,
}

#[derive(Debug)]
pub enum NewSubscriberError {
    InvalidEmail(String),
    InvalidName(String),
    InvalidToken(String),
}

impl std::fmt::Display for NewSubscriberError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NewSubscriberError::InvalidEmail(email) => {
                write!(f, "{} is not a valid subscriber email.", email)
            }
            NewSubscriberError::InvalidName(name) => {
                write!(f, "{} is not a valid subscriber name.", name)
            }
            NewSubscriberError::InvalidToken(token) => {
                write!(f, "{} is not a valid subscriber token.", token)
            }
        }
    }
}

impl std::error::Error for NewSubscriberError {}
