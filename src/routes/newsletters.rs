//! src/routes/newsletters.rs

use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::error::{Error, Z2PResult};
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
    email: SubscriberEmail,
}

pub async fn publish_newsletter(
    body: web::Json<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Z2PResult<HttpResponse> {
    let subscribers = get_confirmed_subscribers(&pool).await?;
    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &body.title,
                        &body.content.html,
                        &body.content.text,
                    )
                    .await?;
            }
            Err(err) => {
                tracing::warn!(
                    // We record the error chain as a structured field on the log record
                    error.cause_chain = ?err,
                    // Using `\` to split a long string literal over
                    // two lines, without creating a `\n` character
                    "Skiping a confirmed subscriber. \
                    Thier stored contact details are invalid.",
                );
            }
        }
    }
    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(
    pool: &PgPool,
) -> Z2PResult<Vec<Z2PResult<ConfirmedSubscriber>>> {
    let confirmed_subscribers = sqlx::query!(
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = $1
        "#,
        SubscriptionsStatus::Confirmed as SubscriptionsStatus,
    )
    .fetch_all(pool)
    .await
    .context("Failed to read confirmed subscribers from database.")?
    .into_iter()
    .map(|r| {
        SubscriberEmail::parse(r.email)
            .map(|email| ConfirmedSubscriber { email })
            .map_err(Error::from)
    })
    .collect();
    Ok(confirmed_subscribers)
}
