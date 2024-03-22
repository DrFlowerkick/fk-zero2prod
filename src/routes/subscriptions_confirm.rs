//! src/routes/subscriptions_confirm.rs

use crate::app_error::AppError;
use crate::domain::{NewSubscriberError, SubscriberToken};
use crate::routes::get_status_from_subscriber_id;
use actix_web::{web, HttpResponse};
use anyhow::Context;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Serialize, serde::Deserialize, Debug, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "subscriptions_status", rename_all = "snake_case")]
pub enum SubscriptionsStatus {
    PendingConfirmation,
    Confirmed,
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(subscriber_token, pool))]
pub async fn confirm(
    subscriber_token: web::Query<SubscriberToken>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, AppError> {
    subscriber_token.is_valid()?;
    let id = get_subscriber_id_from_token(&pool, &subscriber_token).await?;
    match id {
        // Non-existing token!
        None => Err(NewSubscriberError::InvalidToken(
            subscriber_token.as_ref().to_owned(),
        ))?,
        Some(subscriber_id) => {
            if confirm_subscriber(&pool, subscriber_id).await? {
                Ok(HttpResponse::Ok()
                    .json("status changed from pending_confirmation to confirmed."))
            } else {
                Ok(HttpResponse::Ok().finish())
            }
        }
    }
}

#[tracing::instrument(name = "Mark subscriber as confirmed", skip(subscriber_id, pool))]
pub async fn confirm_subscriber(pool: &PgPool, subscriber_id: Uuid) -> Result<bool, AppError> {
    // check status of entry with subscriber_id
    match get_status_from_subscriber_id(pool, subscriber_id).await? {
        SubscriptionsStatus::PendingConfirmation => {
            // Update status to confirmed
            sqlx::query!(
                r#"UPDATE subscriptions SET status = $1 WHERE id = $2"#,
                SubscriptionsStatus::Confirmed as SubscriptionsStatus,
                subscriber_id,
            )
            .execute(pool)
            .await
            .context(
                "Failed to update status of subscriber_id for confirmation of subscription.",
            )?;
            Ok(true)
        }
        // subscription is already confirmed
        SubscriptionsStatus::Confirmed => Ok(false),
    }
}

#[tracing::instrument(name = "Get subscriber_id from token", skip(subscription_token, pool))]
pub async fn get_subscriber_id_from_token(
    pool: &PgPool,
    subscription_token: &SubscriberToken,
) -> Result<Option<Uuid>, AppError> {
    let result = sqlx::query!(
        "SELECT subscriber_id FROM subscription_tokens \
        WHERE subscription_token = $1",
        subscription_token.as_ref(),
    )
    .fetch_optional(pool)
    .await
    .context("Failed to read subscriber_id of subscription_token from database.")?;
    Ok(result.map(|r| r.subscriber_id))
}
