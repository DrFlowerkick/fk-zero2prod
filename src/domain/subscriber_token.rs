//! src/domain/subscriber_token.rs

use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use crate::domain::NewSubscriberError;

#[derive(serde::Deserialize, Debug, Clone)]
pub struct SubscriberToken {
    subscription_token: String,
}

impl AsRef<str> for SubscriberToken {
    fn as_ref(&self) -> &str {
        &self.subscription_token
    }
}

impl SubscriberToken {
    /// Generate a random 25-characters-long case-sensitive subscription token.
    pub fn generate_subscription_token() -> Self {
        let mut rng = thread_rng();
        Self {
            subscription_token: std::iter::repeat_with(|| rng.sample(Alphanumeric))
                .map(char::from)
                .take(25)
                .collect()
        }
    }
    /// check if any char of subscription_token is not alphanumeric
    pub fn is_valid(&self) -> Result<&str, NewSubscriberError> {
        if self
            .subscription_token
            .chars()
            .any(|c| !c.is_alphanumeric()) || self.subscription_token.chars().count() != 25 {
            Err(NewSubscriberError::InvalidToken(self.subscription_token.to_owned()))
        } else {
            Ok(&self.subscription_token)
        }
    }
    /// parse string as token
    pub fn parse(s: String) -> Result<SubscriberToken, NewSubscriberError> {
        let subscription_token = Self { subscription_token: s };
        subscription_token.is_valid()?;
        Ok(subscription_token)
    }
}