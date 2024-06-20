//! src/app_error.rs

use crate::authentication::CredentialsError;
use crate::domain::ValidationError;
use crate::routes::NewsletterError;
use crate::session_state::SessionError;
use crate::utils::see_other;
use actix_web_flash_messages::FlashMessage;

pub type Z2PResult<T> = Result<T, Error>;

pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

#[derive(thiserror::Error)]
pub enum Error {
    #[error("Invalid input for subscription")]
    SubscriptionError(#[from] ValidationError),
    #[error("Failed Login Authentication")]
    LoginError,
    #[error("Failure changing password")]
    PasswordChangingError(#[from] CredentialsError),
    #[error("Unvalid input for Newsletter")]
    NewsletterError(#[from] NewsletterError),
    #[error("Session state error")]
    SessionStateError(#[from] SessionError),
    #[error("Wrong format of idempotency key")]
    IdempotencyKeyError,
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl From<Error> for actix_web::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::SubscriptionError(ref valerr) => {
                FlashMessage::error(valerr.to_string()).send();
                let response = see_other("/subscriptions");
                actix_web::error::InternalError::from_response(err, response).into()
            }
            Error::IdempotencyKeyError => {
                actix_web::error::ErrorBadRequest(err)
            }
            Error::LoginError | Error::SessionStateError(_) => {
                FlashMessage::error(err.to_string()).send();
                let response = see_other("/login");
                actix_web::error::InternalError::from_response(err, response).into()
            }
            Error::PasswordChangingError(CredentialsError::UnexpectedError(_)) => {
                actix_web::error::ErrorInternalServerError(err)
            }
            Error::PasswordChangingError(ref pcerr) => {
                FlashMessage::error(pcerr.to_string()).send();
                let response = see_other("/admin/password");
                actix_web::error::InternalError::from_response(err, response).into()
            }
            Error::NewsletterError(ref nwerr) => {
                FlashMessage::error(nwerr.to_string()).send();
                let response = see_other("/admin/newsletters");
                actix_web::error::InternalError::from_response(err, response).into()
            }
            Error::UnexpectedError(_) => actix_web::error::ErrorInternalServerError(err),
        }
    }
}
