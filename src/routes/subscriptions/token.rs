//! src/routes/subscriptions/token.rs

use actix_web::Responder;
use actix_web_flash_messages::IncomingFlashMessages;
use askama_actix::Template;

#[derive(Template)]
#[template(path = "subscriptions_token.html")]
struct SubscriptionsTokenTemplate {
    flash_messages: Vec<String>,
}

pub async fn subscription_token(flash_messages: IncomingFlashMessages) -> impl Responder {
    let flash_messages: Vec<String> = flash_messages
        .iter()
        .map(|m| m.content().to_string())
        .collect();
    SubscriptionsTokenTemplate { flash_messages }
}
