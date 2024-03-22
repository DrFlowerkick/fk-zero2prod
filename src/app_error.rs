//! src/app_error.rs

use crate::domain::NewSubscriberError;
use actix_web::http::StatusCode;
use actix_web::ResponseError;

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
pub enum AppError {
    #[error("Invalid input for subscription")]
    SubscriptionError(#[from] NewSubscriberError),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for AppError {
    fn status_code(&self) -> reqwest::StatusCode {
        match self {
            AppError::SubscriptionError(_) => StatusCode::BAD_REQUEST,
            AppError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
