//! src/issue_delivery_worker.rs

use crate::{
    configuration::Settings,
    email_client::EmailClient,
    error::{Error, Z2PResult},
    routes::get_subscriber_from_subscriber_id,
    startup::get_connection_pool,
};
use anyhow::Context;
use askama::Template;
use chrono::{DateTime, Utc};
use sqlx::{Executor, PgPool, Postgres, Row, Transaction};
use std::time::Duration;
use tracing::{field::display, Span};
use uuid::Uuid;

pub async fn run_delivery_worker_until_stopped(configuration: Settings) -> Z2PResult<()> {
    let connection_pool = get_connection_pool(&configuration.database);
    let max_retries = configuration.emailclient.n_retries;
    let time_delta = chrono::TimeDelta::milliseconds(
        configuration.emailclient.execute_retry_after_milliseconds as i64,
    );
    let base_url = configuration.application.base_url;
    let email_client = configuration.emailclient.client();
    worker_loop(
        connection_pool,
        email_client,
        max_retries,
        time_delta,
        &base_url,
    )
    .await
}

async fn worker_loop(
    pool: PgPool,
    email_client: EmailClient,
    max_retries: u8,
    time_delta: chrono::TimeDelta,
    base_url: &str,
) -> Z2PResult<()> {
    let mut wait_postponed_tasks: u64 = 10;
    loop {
        match try_execute_task(&pool, &email_client, max_retries, time_delta, base_url).await {
            Ok(ExecutionOutcome::EmptyQueue) => {
                tokio::time::sleep(Duration::from_secs(10)).await;
                wait_postponed_tasks = 10;
            }
            Ok(ExecutionOutcome::PostponedTasks) => {
                // wait a short time and check again for unlocked tasks
                // increase sleep time for each loop up to 10 seconds
                // reset time to 10 ms at any other result.
                tokio::time::sleep(Duration::from_millis(wait_postponed_tasks)).await;
                if wait_postponed_tasks < 10_000 {
                    wait_postponed_tasks *= 10;
                }
            }
            Err(_) => {
                // sleep one second and try to recover from transient errors
                tokio::time::sleep(Duration::from_secs(1)).await;
                wait_postponed_tasks = 10;
            }
            Ok(ExecutionOutcome::TaskCompleted) => {
                wait_postponed_tasks = 10;
            }
        }
    }
}

pub enum ExecutionOutcome {
    TaskCompleted,
    EmptyQueue,
    PostponedTasks,
}

#[derive(Template)]
#[template(path = "email_newsletter.html")]
struct EmailHtmlTemplate<'a> {
    title: &'a str,
    name: &'a str,
    content: &'a str,
    unsubscribe_link: &'a str,
}

#[derive(Template)]
#[template(path = "email_newsletter.txt")]
struct EmailTextTemplate<'a> {
    title: &'a str,
    name: &'a str,
    content: &'a str,
    unsubscribe_link: &'a str,
}

#[tracing::instrument(
    skip_all,
    fields(
        newsletter_issue_id=tracing::field::Empty,
        subscriber_email=tracing::field::Empty
    )
)]
pub async fn try_execute_task(
    pool: &PgPool,
    email_client: &EmailClient,
    max_retries: u8,
    time_delta: chrono::TimeDelta,
    base_url: &str,
) -> Z2PResult<ExecutionOutcome> {
    let task = dequeue_task(pool).await?;
    if task.is_none() {
        if is_task_queue_empty(pool).await? {
            return Ok(ExecutionOutcome::EmptyQueue);
        } else {
            return Ok(ExecutionOutcome::PostponedTasks);
        }
    }
    let (transaction, issue_id, user_id, n_retries, execute_after) = task.unwrap();
    Span::current().record("newsletter_issue_id", &display(issue_id));
    match get_subscriber_from_subscriber_id(pool, user_id).await {
        Ok((parsed_name, parsed_email, parsed_token, _)) => {
            Span::current()
                .record("subscriber_name", &display(parsed_name.as_ref()))
                .record("subscriber_email", &display(parsed_email.as_ref()));
            let issue = get_issue(pool, issue_id).await?;
            // We create a unsubscribe link
            let unsubscribe_link = format!(
                "{}/subscriptions/unsubscribe?subscription_token={}",
                base_url,
                parsed_token.as_ref()
            );

            let plain_body = EmailTextTemplate {
                title: &issue.title,
                name: parsed_name.as_ref(),
                content: &issue.text_content,
                unsubscribe_link: unsubscribe_link.as_ref(),
            }
            .render()
            .context("Failed to render html body.")?;
            let html_body = EmailHtmlTemplate {
                title: &issue.title,
                name: parsed_name.as_ref(),
                content: &issue.html_content,
                unsubscribe_link: unsubscribe_link.as_ref(),
            }
            .render()
            .context("Failed to render html body.")?;
            if let Err(e) = email_client
                .send_email(&parsed_email, &issue.title, &html_body, &plain_body)
                .await
            {
                if n_retries >= max_retries {
                    tracing::error!(
                        error.cause_chain = ?e,
                        error.message = %e,
                        "Failed to deliver issue to a confirmed subscriber. Skipping.",
                    );
                    update_issue_delivery_failure(pool, issue_id).await?;
                    delete_task(transaction, issue_id, user_id).await?;
                } else {
                    let update_execute_after_timestamp = execute_after
                        .checked_add_signed(time_delta)
                        .ok_or(anyhow::anyhow!("failed to add time_delta"))?;
                    update_execute_after_of_task(
                        transaction,
                        issue_id,
                        user_id,
                        n_retries,
                        update_execute_after_timestamp,
                    )
                    .await?;
                }
            } else {
                update_issue_delivery_success(pool, issue_id).await?;
                delete_task(transaction, issue_id, user_id).await?;
            }
        }
        Err(Error::SubscriptionError(e)) => {
            // ValidationError is fatal and cannot be recoverd.
            // Task is completed.
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Skipping a confirmed subscriber. \
                Thier stored contact details are invalid.",
            );
            update_issue_delivery_failure(pool, issue_id).await?;
            delete_task(transaction, issue_id, user_id).await?;
        }

        Err(e) => {
            // unexpected transient err
            Err(e)?;
        }
    }
    Ok(ExecutionOutcome::TaskCompleted)
}

pub type PgTransaction = Transaction<'static, Postgres>;
type TaskData = (PgTransaction, Uuid, Uuid, u8, DateTime<Utc>);

#[tracing::instrument(skip_all)]
async fn dequeue_task(pool: &PgPool) -> Result<Option<TaskData>, anyhow::Error> {
    let mut transaction: PgTransaction = pool.begin().await?;
    let query = sqlx::query!(
        r#"
        SELECT newsletter_issue_id, user_id, n_retries, execute_after
        FROM issue_delivery_queue
        WHERE NOW() > execute_after
        FOR UPDATE
        SKIP LOCKED
        LIMIT 1
        "#,
    );
    let r = transaction.fetch_optional(query).await?;
    if let Some(r) = r {
        let n_retries: i16 = r.try_get("n_retries")?;
        if n_retries < 0 {
            Err(anyhow::anyhow!("value n_retries < 0"))?;
        }
        Ok(Some((
            transaction,
            r.try_get("newsletter_issue_id")?,
            r.try_get("user_id")?,
            n_retries as u8,
            r.try_get("execute_after")?,
        )))
    } else {
        Ok(None)
    }
}

#[tracing::instrument(skip_all)]
async fn is_task_queue_empty(pool: &PgPool) -> Result<bool, anyhow::Error> {
    // Prepare the query to count rows in the specified table
    let query = "SELECT COUNT(*) as count FROM issue_delivery_queue".to_string();

    // Execute the query
    let row = sqlx::query(&query).fetch_one(pool).await?;

    // Extract the count from the row
    let count: i64 = row.try_get("count")?;

    // Check if the count is 0
    Ok(count == 0)
}

#[tracing::instrument(skip_all)]
async fn delete_task(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    user_id: Uuid,
) -> Result<(), anyhow::Error> {
    let query = sqlx::query!(
        r#"
        DELETE FROM issue_delivery_queue
        WHERE
            newsletter_issue_id = $1 AND
            user_id = $2
        "#,
        issue_id,
        user_id
    );
    transaction.execute(query).await?;
    transaction.commit().await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn update_execute_after_of_task(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    user_id: Uuid,
    n_retries: u8,
    update_execute_after_timestamp: DateTime<Utc>,
) -> Result<(), anyhow::Error> {
    let query = sqlx::query!(
        r#"
        UPDATE issue_delivery_queue
        SET
            n_retries = $3,
            execute_after = $4
        WHERE
            newsletter_issue_id = $1 AND
            user_id = $2
        "#,
        issue_id,
        user_id,
        (n_retries + 1) as i16,
        update_execute_after_timestamp
    );
    transaction.execute(query).await?;
    transaction.commit().await?;
    Ok(())
}

struct NewsletterIssue {
    title: String,
    text_content: String,
    html_content: String,
}

#[tracing::instrument(skip_all)]
async fn get_issue(pool: &PgPool, issue_id: Uuid) -> Result<NewsletterIssue, anyhow::Error> {
    let issue = sqlx::query_as!(
        NewsletterIssue,
        r#"
        SELECT title, text_content, html_content
        FROM newsletter_issues
        WHERE
            newsletter_issue_id = $1
        "#,
        issue_id
    )
    .fetch_one(pool)
    .await?;
    Ok(issue)
}

#[tracing::instrument(skip_all)]
async fn update_issue_delivery_success(pool: &PgPool, issue_id: Uuid) -> Result<(), anyhow::Error> {
    let mut transaction: Transaction<'_, Postgres> = pool.begin().await?;
    let query = sqlx::query!(
        r#"
        SELECT num_delivered_newsletters
        FROM newsletter_issues
        WHERE
            newsletter_issue_id = $1
        FOR UPDATE;
        "#,
        issue_id
    );
    let row = transaction.fetch_one(query).await?;

    let num_delivered_newsletters: i32 = row.try_get("num_delivered_newsletters")?;

    let query = sqlx::query!(
        r#"
        UPDATE newsletter_issues
        SET
            num_delivered_newsletters = $2
        WHERE
            newsletter_issue_id = $1
        "#,
        issue_id,
        num_delivered_newsletters + 1
    );
    transaction.execute(query).await?;

    transaction.commit().await?;

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn update_issue_delivery_failure(pool: &PgPool, issue_id: Uuid) -> Result<(), anyhow::Error> {
    let mut transaction: Transaction<'_, Postgres> = pool.begin().await?;
    let query = sqlx::query!(
        r#"
        SELECT num_failed_deliveries
        FROM newsletter_issues
        WHERE
            newsletter_issue_id = $1
        FOR UPDATE;
        "#,
        issue_id
    );
    let row = transaction.fetch_one(query).await?;

    let num_failed_deliveries: i32 = row.try_get("num_failed_deliveries")?;

    let query = sqlx::query!(
        r#"
        UPDATE newsletter_issues
        SET
        num_failed_deliveries = $2
        WHERE
            newsletter_issue_id = $1
        "#,
        issue_id,
        num_failed_deliveries + 1
    );
    transaction.execute(query).await?;

    transaction.commit().await?;

    Ok(())
}
