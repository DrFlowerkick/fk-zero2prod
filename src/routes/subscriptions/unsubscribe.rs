//! src/routes/subscriptions_confirm.rs

use crate::domain::{SubscriberToken, ValidationError};
use crate::error::Z2PResult;
use crate::issue_delivery_worker::PgTransaction;
use crate::routes::{get_subscriber_from_subscriber_id, get_subscriber_id_from_token};
use actix_web::{web, Responder};
use anyhow::Context;
use askama_actix::Template;
use sqlx::{Executor, PgPool};
use uuid::Uuid;

#[derive(Template)]
#[template(path = "subscriptions_unsubscribe.html")]
struct UnsubscribeTemplate {
    name: String,
    email: String,
}

#[tracing::instrument(name = "Confirm unsubscribe subscriber", skip(subscriber_token, pool))]
pub async fn unsubscribe(
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
            let (name, email, ..) = get_subscriber_from_subscriber_id(&pool, subscriber_id).await?;
            remove_subscriber_from_database(&pool, subscriber_id).await?;
            Ok(UnsubscribeTemplate {
                name: name.as_ref().to_owned(),
                email: email.as_ref().to_owned(),
            })
        }
    }
}

#[tracing::instrument(name = "Remove subscriber and token from database", skip_all)]
async fn remove_subscriber_from_database(pool: &PgPool, subscriber_id: Uuid) -> Z2PResult<()> {
    // start transaction
    let mut transaction: PgTransaction = pool
        .begin()
        .await
        .context("Failed to create transaction.")?;
    // remove token
    let query = sqlx::query!(
        r#"
        DELETE FROM subscription_tokens
        WHERE
            subscriber_id = $1
        "#,
        subscriber_id
    );
    transaction
        .execute(query)
        .await
        .context("Failed to execute query to remove token")?;
    // remove subscriber
    let query = sqlx::query!(
        r#"
        DELETE FROM subscriptions
        WHERE
            id = $1
        "#,
        subscriber_id
    );
    transaction
        .execute(query)
        .await
        .context("Failed to execute query to remove subscriber")?;
    // commit transaction
    transaction
        .commit()
        .await
        .context("Failed to commit transaction")?;
    Ok(())
}
