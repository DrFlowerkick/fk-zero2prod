//! src/routes/subscriptions_confirm.rs

use crate::routes::get_status_from_subscriber_id;
use actix_web::{web, HttpResponse};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(serde::Serialize, serde::Deserialize, Debug, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "subscriptions_status", rename_all = "snake_case")]
pub enum SubscriptionsStatus {
    PendingConfirmation,
    Confirmed,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Parameters {
    subscription_token: String,
}

impl Parameters {
    pub fn is_valid(&self) -> bool {
        // check if any char of subscription_token is not alphanumeric
        !self
            .subscription_token
            .chars()
            .any(|c| !c.is_alphanumeric())
    }
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(parameters, pool))]
pub async fn confirm(parameters: web::Query<Parameters>, pool: web::Data<PgPool>) -> HttpResponse {
    if !parameters.is_valid() {
        return HttpResponse::BadRequest().finish();
    }
    let id = match get_subscriber_id_from_token(&pool, &parameters.subscription_token).await {
        Ok(id) => id,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    match id {
        // Non-existing token!
        None => HttpResponse::NotFound().finish(),
        Some(subscriber_id) => match confirm_subscriber(&pool, subscriber_id).await {
            Err(_) => HttpResponse::InternalServerError().finish(),
            Ok(new_confirmation) => {
                if new_confirmation {
                    HttpResponse::Ok()
                        .json("status changed from pending_confirmation to confirmed.")
                } else {
                    HttpResponse::Ok().finish()
                }
            }
        },
    }
}

#[tracing::instrument(name = "Mark subscriber as confirmed", skip(subscriber_id, pool))]
pub async fn confirm_subscriber(pool: &PgPool, subscriber_id: Uuid) -> Result<bool, sqlx::Error> {
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
            .map_err(|e| {
                tracing::error!("Failed to execute query: {:?}", e);
                e
            })?;
            Ok(true)
        }
        // subscription is already confirmed
        SubscriptionsStatus::Confirmed => Ok(false),
    }
}

#[tracing::instrument(name = "Get subscriber_id from token", skip(subscription_token, pool))]
pub async fn get_subscriber_id_from_token(
    pool: &PgPool,
    subscription_token: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT subscriber_id FROM subscription_tokens \
        WHERE subscription_token = $1",
        subscription_token,
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(result.map(|r| r.subscriber_id))
}
