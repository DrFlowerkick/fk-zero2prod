//! src/app_error.rs

use crate::domain::ValidationError;
use actix_web::http::{StatusCode, header, header::HeaderValue};
use actix_web::{HttpResponse, ResponseError};

pub type Z2PResult<T> = Result<T, Error>;

fn error_chain_fmt(
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
    #[error("Authentification failed")]
    AuthError(#[source] anyhow::Error),
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
            Error::SubscriptionError(_) => {
                HttpResponse::new(StatusCode::BAD_REQUEST)
            },
            Error::AuthError(_) => {
                let mut response = HttpResponse::new(StatusCode::UNAUTHORIZED);
                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();
                response
                    .headers_mut()
                    // actix_web::http::header provides a collection of constants
                    // for the names of several well-known/standard HTTP headers
                    .insert(header::WWW_AUTHENTICATE, header_value);
                response
            },
            Error::UnexpectedError(_) => {
                HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR)
            },
        }
    }
}

impl Error {
    pub fn convert_unexpected_to_auth_error(self) -> Self {
        match self {
            Error::UnexpectedError(err) => Error::AuthError(err),
            _ => self
        }
    }
}