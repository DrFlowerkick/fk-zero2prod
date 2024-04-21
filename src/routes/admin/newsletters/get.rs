//! src/routes/admin/newsletters/get.rs

use actix_web::Responder;
use askama_actix::Template;

#[derive(Template)]
#[template(path = "newsletters.html")]
struct NewslettersTemplate {}

pub async fn publish_newsletter_form() -> impl Responder {
    NewslettersTemplate{}
}
