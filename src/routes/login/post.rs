//! src/routes/login/post.rs

use actix_web::{
    HttpResponse,
    http::header::LOCATION,
    web
};
use secrecy::Secret;

pub async fn login(_form: web::Form<FormData>) -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header((LOCATION, "/"))
        .finish()
}

#[derive(serde::Deserialize)]
pub struct FormData {
    username: String,
    password: Secret<String>,
}