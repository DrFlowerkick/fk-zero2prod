//! src/routes/admin/dashboard.rs

use actix_web::{web, Responder};
use askama_actix::Template;
use sqlx::PgPool;

use crate::authentication::UserId;
use crate::error::Z2PResult;

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    username: String,
}

pub async fn admin_dashboard(
    pool: web::Data<PgPool>,
    user_id: web::ReqData<UserId>,
) -> Z2PResult<impl Responder> {
    let username = user_id.get_username(&pool).await?;
    Ok(DashboardTemplate { username })
}
