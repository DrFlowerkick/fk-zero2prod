//! src/idempotency/key.rs

use crate::error::{Error, Z2PResult};
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug)]
pub struct IdempotencyKey(String);

impl TryFrom<String> for IdempotencyKey {
    type Error = Error;

    fn try_from(s: String) -> Z2PResult<Self> {
        if Uuid::from_str(&s).is_err() {
            return Err(Error::IdempotencyKeyError);
        }
        Ok(Self(s))
    }
}

impl From<IdempotencyKey> for String {
    fn from(value: IdempotencyKey) -> Self {
        value.0
    }
}

impl AsRef<str> for IdempotencyKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
