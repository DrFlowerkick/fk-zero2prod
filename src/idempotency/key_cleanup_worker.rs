//! src/idempotency/key_cleanup_worker.rs

use crate::{configuration::Settings, error::Z2PResult, startup::get_connection_pool};
use anyhow::Context;
use sqlx::PgPool;
use std::time::Duration;

pub async fn run_cleanup_worker_until_stopped(configuration: Settings) -> Z2PResult<()> {
    let connection_pool = get_connection_pool(&configuration.database);

    worker_loop(
        connection_pool,
        configuration.application.idempotency_lifetime_minutes,
    )
    .await
}

async fn worker_loop(pool: PgPool, lifetime_minutes: u32) -> Z2PResult<()> {
    loop {
        delete_outlived_idempotency_key(&pool, lifetime_minutes).await?;
        tokio::time::sleep(Duration::from_secs(600)).await;
    }
}

pub async fn delete_outlived_idempotency_key(
    pool: &PgPool,
    lifetime_minutes: u32,
) -> Z2PResult<u64> {
    let query = format!(
        "DELETE FROM idempotency WHERE created_at < NOW() - INTERVAL '{} minutes'",
        lifetime_minutes
    );
    let delete_result = sqlx::query(&query)
        .execute(pool)
        .await
        .context("Could not execute query to delete idempotency keys.")?;

    Ok(delete_result.rows_affected())
}
