//! src/app_error.rs

use crate::authentication::AuthError;
use crate::domain::ValidationError;
use actix_web::http::{header, header::HeaderValue, StatusCode};
use actix_web::{HttpResponse, ResponseError};

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
    #[error("Bad Request authentication header.")]
    BadRequestAuthHeader(#[source] anyhow::Error),
    #[error("Failed Basic Authentication")]
    BasicAuthError(#[source] anyhow::Error),
    #[error("Failed Login Authentication")]
    LoginError(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for Error {
    fn error_response(&self) -> HttpResponse {
        match self {
            Error::SubscriptionError(_) => HttpResponse::new(StatusCode::BAD_REQUEST),
            Error::BasicAuthError(_) | Error::BadRequestAuthHeader(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();
                response
                    .headers_mut()
                    // actix_web::http::header provides a collection of constants
                    // for the names of several well-known/standard HTTP headers
                    .insert(header::WWW_AUTHENTICATE, header_value);
                response
            }
            Error::LoginError(_) => HttpResponse::new(StatusCode::BAD_REQUEST),
            Error::UnexpectedError(_) => HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }
}

impl Error {
    pub fn auth_error_to_basic_auth_error(err: AuthError) -> Self {
        match err {
            AuthError::InvalidCreds(err) => Self::BasicAuthError(err),
            AuthError::UnexpectedError(err) => Self::UnexpectedError(err),
        }
    }
    pub fn auth_error_to_login_error(err: AuthError) -> Self {
        match err {
            AuthError::InvalidCreds(err) => Self::LoginError(err),
            AuthError::UnexpectedError(err) => Self::UnexpectedError(err),
        }
    }
}
