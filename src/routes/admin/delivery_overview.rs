//! src/routes/admin/delivery_overview.rs

use actix_web::{web, Responder};
use anyhow::Context;
use askama_actix::Template;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Z2PResult;

#[derive(Template)]
#[template(path = "delivery_overview.html")]
struct DeliveryOverview {
    issue_to_display: Option<NewsletterIssue>,
    newsletters: Vec<NewsletterIssue>,
}

#[derive(Clone, Debug)]
struct NewsletterIssue {
    newsletter_issue_id: Uuid,
    title: String,
    text_content: String,
    html_content: String,
    published_at: DateTime<Utc>,
    num_current_subscribers: Option<i32>,
    num_delivered_newsletters: Option<i32>,
    num_failed_deliveries: Option<i32>,
}

#[derive(serde::Deserialize, Debug)]
pub struct QueryData {
    newsletter_issue_id: Uuid,
}

pub async fn delivery_overview(
    query: Option<web::Query<QueryData>>,
    pool: web::Data<PgPool>,
) -> Z2PResult<impl Responder> {
    let newsletters = get_newsletters_info(&pool)
        .await
        .context("Failed to read infos of all newsletters")?;
    let issue_to_display = if let Some(f) = query {
        newsletters
            .iter()
            .find(|n| n.newsletter_issue_id == f.newsletter_issue_id)
            .cloned()
    } else {
        None
    };
    Ok(DeliveryOverview {
        issue_to_display,
        newsletters,
    })
}

#[tracing::instrument(skip_all)]
async fn get_newsletters_info(pool: &PgPool) -> Result<Vec<NewsletterIssue>, sqlx::Error> {
    let newsletters_info = sqlx::query_as!(
        NewsletterIssue,
        r#"
        SELECT newsletter_issue_id, title, text_content, html_content, published_at, num_current_subscribers, num_delivered_newsletters, num_failed_deliveries
        FROM newsletter_issues
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(newsletters_info)
}
