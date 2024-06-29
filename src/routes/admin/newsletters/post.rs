//! src/routes/admin/newsletters/post.rs

use actix_web::web::ReqData;
use actix_web::{web, HttpResponse};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context;
use sqlx::{Executor, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::authentication::UserId;
use crate::error::{error_chain_fmt, Z2PResult};
use crate::idempotency::{save_response, try_processing, IdempotencyKey, NextAction};
use crate::routes::SubscriptionsStatus;
use crate::utils::see_other;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct NewsletterFormData {
    pub title: String,
    pub html_content: String,
    pub text_content: String,
    pub idempotency_key: String,
}

#[derive(thiserror::Error)]
pub enum NewsletterError {
    #[error("You must set a title for your newsletter.")]
    NoTitle,
    #[error("You must set text content for your newsletter.")]
    NoTextContent,
    #[error("You must set html content for your newsletter.")]
    NoHtmlContent,
}

impl std::fmt::Debug for NewsletterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

#[tracing::instrument(
    name = "Publish a newsletter issue",
    skip_all,
    fields(user_id=%&*user_id)
)]
pub async fn publish_newsletter(
    form: web::Form<NewsletterFormData>,
    pool: web::Data<PgPool>,
    user_id: ReqData<UserId>,
) -> Z2PResult<HttpResponse> {
    if form.0.title.is_empty() {
        Err(NewsletterError::NoTitle)?;
    }
    if form.0.text_content.is_empty() {
        Err(NewsletterError::NoTextContent)?;
    }
    if form.0.html_content.is_empty() {
        Err(NewsletterError::NoHtmlContent)?;
    }
    let user_id = user_id.into_inner();
    // We must destructure the form to avoid upsetting the borrow-checker
    let NewsletterFormData {
        title,
        html_content,
        text_content,
        idempotency_key,
    } = form.0;

    let idempotency_key: IdempotencyKey = idempotency_key.try_into()?;
    let mut transaction = match try_processing(&pool, &idempotency_key, *user_id).await? {
        NextAction::StartProcessing(t) => t,
        NextAction::ReturnSavedResponse(saved_response) => {
            success_message().send();
            return Ok(saved_response);
        }
    };
    let issue_id = insert_newsletter_issue(&mut transaction, &title, &text_content, &html_content)
        .await
        .context("Failed to store newsletter issue details")?;
    let num_current_subscribers = enqueue_delivery_tasks(&mut transaction, issue_id)
        .await
        .context("Failed to enqueue delivera tasks")?;
    initialize_newsletter_delivery_data(&mut transaction, issue_id, num_current_subscribers)
        .await
        .context("Failed to initialize newsletter delivery overview")?;

    let response = see_other("/admin/newsletters");
    let response = save_response(transaction, &idempotency_key, *user_id, response).await?;
    success_message().send();
    Ok(response)
}

fn success_message() -> FlashMessage {
    FlashMessage::info("The newsletter issue has been accepted - emails will go out shortly.")
}

#[tracing::instrument(skip_all)]
async fn insert_newsletter_issue(
    transaction: &mut Transaction<'_, Postgres>,
    title: &str,
    text_content: &str,
    html_content: &str,
) -> Result<Uuid, sqlx::Error> {
    let newsletter_issue_id = Uuid::new_v4();
    let query = sqlx::query!(
        r#"
        INSERT INTO newsletter_issues (
            newsletter_issue_id,
            title,
            text_content,
            html_content,
            published_at
        )
        VALUES ($1, $2, $3, $4, now())
        "#,
        newsletter_issue_id,
        title,
        text_content,
        html_content
    );
    transaction.execute(query).await?;
    Ok(newsletter_issue_id)
}

#[tracing::instrument(skip_all)]
async fn enqueue_delivery_tasks(
    transaction: &mut Transaction<'_, Postgres>,
    newsletter_issue_id: Uuid,
) -> Result<i32, sqlx::Error> {
    let query = sqlx::query!(
        r#"
        INSERT INTO issue_delivery_queue (
            newsletter_issue_id,
            user_id,
            n_retries,
            execute_after
        )
        SELECT $1, id, 0, NOW()
        FROM subscriptions
        WHERE status = $2
        "#,
        newsletter_issue_id,
        SubscriptionsStatus::Confirmed as SubscriptionsStatus,
    );
    let num_current_subscribers = transaction.execute(query).await?.rows_affected() as i32;
    Ok(num_current_subscribers)
}

#[tracing::instrument(skip_all)]
async fn initialize_newsletter_delivery_data(
    transaction: &mut Transaction<'_, Postgres>,
    newsletter_issue_id: Uuid,
    num_current_subscribers: i32,
) -> Result<(), sqlx::Error> {
    let query = sqlx::query!(
        r#"
        UPDATE newsletter_issues
        SET
            num_current_subscribers = $2,
            num_delivered_newsletters = 0,
            num_failed_deliveries = 0
        WHERE
            newsletter_issue_id = $1
        "#,
        newsletter_issue_id,
        num_current_subscribers,
    );
    transaction.execute(query).await?;
    Ok(())
}
