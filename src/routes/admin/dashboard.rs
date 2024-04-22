//! src/routes/admin/dashboard.rs

use actix_web::{web, Responder};
use anyhow::Context;
use askama_actix::Template;
use sqlx::PgPool;
use uuid::Uuid;

use crate::authentication::UserId;
use crate::error::Z2PResult;
use crate::utils::e500;

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    username: String,
}

pub async fn admin_dashboard(
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Result<impl Responder, actix_web::Error> {
    let username = get_username(*user_id.into_inner(), &pool)
        .await
        .map_err(e500)?;
    Ok(DashboardTemplate { username })
}

#[tracing::instrument(name = "Get username", skip(pool))]
pub async fn get_username(user_id: Uuid, pool: &PgPool) -> Z2PResult<String> {
    let row = sqlx::query!(
        r#"
        SELECT username
        FROM users
        WHERE user_id = $1
        "#,
        user_id,
    )
    .fetch_one(pool)
    .await
    .context("Failed to perform a query to retrieve a username.")?;
    Ok(row.username)
}
