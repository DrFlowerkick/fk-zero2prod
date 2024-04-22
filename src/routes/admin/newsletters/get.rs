//! src/routes/admin/newsletters/get.rs

use actix_web::Responder;
use actix_web_flash_messages::IncomingFlashMessages;
use askama_actix::Template;

#[derive(Template)]
#[template(path = "newsletters.html")]
struct NewslettersTemplate {
    flash_messages: Vec<String>,
}

pub async fn publish_newsletter_form(flash_messages: IncomingFlashMessages) -> impl Responder {
    let flash_messages: Vec<String> = flash_messages
        .iter()
        .map(|m| m.content().to_string())
        .collect();
    NewslettersTemplate { flash_messages }
}
