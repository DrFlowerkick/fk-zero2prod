//! src/issue_delivery_worker.rs

use crate::{
    configuration::Settings, domain::SubscriberEmail, email_client::EmailClient, error::Z2PResult,
    startup::get_connection_pool,
};
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

    let email_client = configuration.emailclient.client();
    worker_loop(connection_pool, email_client, max_retries, time_delta).await
}

async fn worker_loop(
    pool: PgPool,
    email_client: EmailClient,
    max_retries: u8,
    time_delta: chrono::TimeDelta,
) -> Z2PResult<()> {
    let mut wait_postponed_tasks: u64 = 10;
    loop {
        match try_execute_task(&pool, &email_client, max_retries, time_delta).await {
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
) -> Z2PResult<ExecutionOutcome> {
    let task = dequeue_task(pool).await?;
    if task.is_none() {
        if is_task_queue_empty(pool).await? {
            return Ok(ExecutionOutcome::EmptyQueue);
        } else {
            return Ok(ExecutionOutcome::PostponedTasks);
        }
    }
    let (transaction, issue_id, email, n_retries, execute_after) = task.unwrap();
    Span::current()
        .record("newsletter_issue_id", &display(issue_id))
        .record("subscriber_email", &display(&email));
    match SubscriberEmail::parse(email.clone()) {
        Ok(parsed_email) => {
            let issue = get_issue(pool, issue_id).await?;
            if let Err(e) = email_client
                .send_email(
                    &parsed_email,
                    &issue.title,
                    &issue.html_content,
                    &issue.text_content,
                )
                .await
            {
                if n_retries >= max_retries {
                    tracing::error!(
                        error.cause_chain = ?e,
                        error.message = %e,
                        "Failed to deliver issue to a confirmed subscriber. Skipping.",
                    );
                    update_issue_delivery_failure(pool, issue_id).await?;
                    delete_task(transaction, issue_id, &email).await?;
                } else {
                    let update_execute_after_timestamp = execute_after
                        .checked_add_signed(time_delta)
                        .ok_or(anyhow::anyhow!("failed to add time_delta"))?;
                    update_execute_after_of_task(
                        transaction,
                        issue_id,
                        &email,
                        n_retries,
                        update_execute_after_timestamp,
                    )
                    .await?;
                }
            } else {
                update_issue_delivery_success(pool, issue_id).await?;
                delete_task(transaction, issue_id, &email).await?;
            }
        }
        Err(e) => {
            // ValidationError is fatal and cannot be recoverd.
            // Task is completed.
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Skipping a confirmed subscriber. \
                Thier stored contact details are unvalid.",
            );
            update_issue_delivery_failure(pool, issue_id).await?;
            delete_task(transaction, issue_id, &email).await?;
        }
    }
    Ok(ExecutionOutcome::TaskCompleted)
}

type PgTransaction = Transaction<'static, Postgres>;
type TaskData = (PgTransaction, Uuid, String, u8, DateTime<Utc>);

#[tracing::instrument(skip_all)]
async fn dequeue_task(pool: &PgPool) -> Result<Option<TaskData>, anyhow::Error> {
    let mut transaction: PgTransaction = pool.begin().await?;
    let query = sqlx::query!(
        r#"
        SELECT newsletter_issue_id, subscriber_email, n_retries, execute_after
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
            r.try_get("subscriber_email")?,
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
    email: &str,
) -> Result<(), anyhow::Error> {
    let query = sqlx::query!(
        r#"
        DELETE FROM issue_delivery_queue
        WHERE
            newsletter_issue_id = $1 AND
            subscriber_email = $2
        "#,
        issue_id,
        email
    );
    transaction.execute(query).await?;
    transaction.commit().await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn update_execute_after_of_task(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    email: &str,
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
            subscriber_email = $2
        "#,
        issue_id,
        email,
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
