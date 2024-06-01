//! src/routes/admin/delivery_overview.rs

use actix_web::{web, Responder};
use anyhow::Context;
use askama_actix::Template;
use sqlx::PgPool;
//use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::utils::e500;

#[derive(Template)]
#[template(path = "delivery_overview.html")]
struct DeliveryOverview {
    newsletters: Vec<NewsletterInfo>,
}

struct NewsletterInfo {
    title: String,
    published_at: DateTime<Utc>,
}

pub async fn delivery_overview(
    pool: web::Data<PgPool>,
) -> Result<impl Responder, actix_web::Error> {
    let newsletters = get_newsletters_info(&pool)
        .await
        .context("Failed to read infos of all newsletters")
        .map_err(e500)?;
    Ok(DeliveryOverview { newsletters })
}

#[tracing::instrument(skip_all)]
async fn get_newsletters_info(pool: &PgPool) -> Result<Vec<NewsletterInfo>, anyhow::Error> {
    let newsletters_info = sqlx::query_as!(
        NewsletterInfo,
        r#"
        SELECT title, published_at
        FROM newsletter_issues
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(newsletters_info)
}
