//! src/routes/admin/newsletters/post.rs

use actix_web::{HttpResponse, web};
use sqlx::PgPool;
use anyhow::Context;

use crate::email_client::EmailClient;
use crate::error::{Z2PResult, Error};
use crate::domain::SubscriberEmail;
use crate::routes::SubscriptionsStatus;

#[derive(serde::Deserialize)]
pub struct FormData {
    title: String,
    html_content: String,
    text_content: String,
}

#[tracing::instrument(
    name = "Publish a newsletter issue",
    skip(form, pool, email_client)
)]
pub async fn publish_newsletter(
    form: web::Form<FormData>,
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
                        &form.title,
                        &form.html_content,
                        &form.text_content,
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




struct ConfirmedSubscriber {
    email: SubscriberEmail,
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