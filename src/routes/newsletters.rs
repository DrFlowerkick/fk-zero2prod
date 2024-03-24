//! src/routes/newsletters.rs

use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::error::Z2PResult;
use crate::routes::SubscriptionsStatus;
use actix_web::{web, HttpResponse};
use anyhow::Context;
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}

#[derive(serde::Deserialize)]
pub struct Content {
    html: String,
    text: String,
}

struct ConfirmedSubscriber {
    email: String,
}

pub async fn publish_newsletter(
    body: web::Json<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Z2PResult<HttpResponse> {
    let subscribers = get_confirmed_subscribers(&pool).await?;
    for subscriber in subscribers {
        let valid_email_from_database = SubscriberEmail::parse(subscriber.email.clone())
            .with_context(|| format!("Read Invalid email {} from database", subscriber.email))?;
        email_client
            .send_email(
                valid_email_from_database,
                &body.title,
                &body.content.html,
                &body.content.text,
            )
            .await?;
    }
    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(pool: &PgPool) -> Z2PResult<Vec<ConfirmedSubscriber>> {
    let rows = sqlx::query_as!(
        ConfirmedSubscriber,
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = $1
        "#,
        SubscriptionsStatus::Confirmed as SubscriptionsStatus,
    )
    .fetch_all(pool)
    .await
    .context("Failed to read confirmed subscribers from database.")?;
    Ok(rows)
}
