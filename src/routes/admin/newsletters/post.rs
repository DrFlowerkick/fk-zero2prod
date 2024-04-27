//! src/routes/admin/newsletters/post.rs

use actix_web::{web, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use sqlx::PgPool;

use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::error::{Error, Z2PResult};
use crate::routes::SubscriptionsStatus;
use crate::utils::{e500, see_other};

#[derive(serde::Deserialize, serde::Serialize)]
pub struct NewsletterFormData {
    pub title: String,
    pub html_content: String,
    pub text_content: String,
    pub idempotency_key: String,
}

#[tracing::instrument(name = "Publish a newsletter issue", skip(form, pool, email_client))]
pub async fn publish_newsletter(
    form: web::Form<NewsletterFormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Result<HttpResponse, actix_web::Error> {
    if form.0.title.is_empty() {
        FlashMessage::error("You must set a title for your newsletter.").send();
        return Ok(see_other("/admin/newsletters"));
    }
    if form.0.html_content.is_empty() && form.0.text_content.is_empty() {
        FlashMessage::error("You must set content for your newsletter.").send();
        return Ok(see_other("/admin/newsletters"));
    }
    // get subscribers
    let subscribers = get_confirmed_subscribers(&pool).await.map_err(e500)?;

    if subscribers.is_empty() {
        FlashMessage::error("You have no confirmed subscribers to send your newsletter to.").send();
        return Ok(see_other("/admin/newsletters"));
    }
    let mut n_invalid_subscriber_emails = 0;
    for subscriber in &subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &form.title,
                        &form.html_content,
                        &form.text_content,
                    )
                    .await
                    .map_err(e500)?;
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
                n_invalid_subscriber_emails += 1;
            }
        }
    }
    if n_invalid_subscriber_emails > 0 {
        FlashMessage::error("You have at least one invalid subscriber. Check your logs.").send();
        return Ok(see_other("/admin/newsletters"));
    }
    FlashMessage::error("Newsletter has been sent.").send();
    Ok(see_other("/admin/newsletters"))
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
