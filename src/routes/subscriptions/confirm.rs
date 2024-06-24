//! src/routes/subscriptions_confirm.rs

use crate::domain::{SubscriberToken, ValidationError};
use crate::error::Z2PResult;
use crate::routes::get_status_from_subscriber_id;
use actix_web::{web, Responder};
use anyhow::Context;
use askama_actix::Template;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Serialize, serde::Deserialize, Debug, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "subscriptions_status", rename_all = "snake_case")]
pub enum SubscriptionsStatus {
    PendingConfirmation,
    Confirmed,
}

#[derive(Template)]
#[template(path = "subscriptions_confirm.html")]
struct SubscriptionsTokenTemplate {
    new_subscription: bool,
    name: String,
    email: String,
    subscribed_at: DateTime<Utc>,
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(subscriber_token, pool))]
pub async fn confirm(
    subscriber_token: web::Query<SubscriberToken>,
    pool: web::Data<PgPool>,
) -> Z2PResult<impl Responder> {
    subscriber_token.is_valid()?;
    let id = get_subscriber_id_from_token(&pool, &subscriber_token).await?;
    match id {
        // Non-existing token!
        None => Err(ValidationError::InvalidToken(
            subscriber_token.as_ref().to_owned(),
        ))?,
        Some(subscriber_id) => {
            let new_subscription = confirm_subscriber(&pool, subscriber_id).await?;
            let (name, email, subscribed_at) =
                get_subscriber_from_subscriber_id(&pool, subscriber_id).await?;
            Ok(SubscriptionsTokenTemplate {
                new_subscription,
                name,
                email,
                subscribed_at,
            })
        }
    }
}

#[tracing::instrument(name = "Mark subscriber as confirmed", skip(subscriber_id, pool))]
pub async fn confirm_subscriber(pool: &PgPool, subscriber_id: Uuid) -> Z2PResult<bool> {
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
) -> Z2PResult<Option<Uuid>> {
    let result = sqlx::query!(
        "SELECT subscriber_id FROM subscription_tokens
        WHERE subscription_token = $1",
        subscription_token.as_ref(),
    )
    .fetch_optional(pool)
    .await
    .context("Failed to read subscriber_id of subscription_token from database.")?;
    Ok(result.map(|r| r.subscriber_id))
}

#[tracing::instrument(
    name = "Get name, eamil and subscribed_at from subscriber_id",
    skip_all
)]
pub async fn get_subscriber_from_subscriber_id(
    pool: &PgPool,
    subscriber_id: Uuid,
) -> Z2PResult<(String, String, DateTime<Utc>)> {
    let result = sqlx::query!(
        "SELECT email, name, subscribed_at FROM subscriptions
        WHERE id = $1",
        subscriber_id,
    )
    .fetch_one(pool)
    .await
    .context("Failed to read subscriber_id of subscription_token from database.")?;
    Ok((result.name, result.email, result.subscribed_at))
}
