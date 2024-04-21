//! src/routes/newsletters.rs

use crate::authentication::{validate_credentials, Credentials};
use crate::domain::SubscriberEmail;
use crate::email_client::EmailClient;
use crate::error::{Error, Z2PResult};
use crate::routes::SubscriptionsStatus;
use actix_web::http::header::HeaderMap;
use actix_web::{web, HttpRequest, HttpResponse};
use anyhow::Context;
use base64::Engine;
use secrecy::Secret;
use sqlx::PgPool;

#[tracing::instrument(
    name = "Publish a newsletter issue",
    skip(body, pool, email_client, request)
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn publish_newsletter(
    body: web::Json<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    request: HttpRequest,
) -> Z2PResult<HttpResponse> {
    // check credentials
    let credentials = basic_authentification(request.headers())?;
    tracing::Span::current().record("username", &tracing::field::display(&credentials.username));
    let user_id = validate_credentials(credentials, &pool)
        .await
        .map_err(Error::auth_error_to_basic_auth_error)?;
    tracing::Span::current().record("user_id", &tracing::field::display(&user_id));
    // send newsletters
    let subscribers = get_confirmed_subscribers(&pool).await?;
    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &body.title,
                        &body.content.html,
                        &body.content.text,
                    )
                    .await?;
            }
            Err(err) => {
                tracing::warn!(
                    // We record the error chain as a structured field on the log record
                    error.cause_chain = ?err,
                    // Using `\` to split a long string literal over
                    // two lines, without creating a `\n` character
                    "Skiping a confirmed subscriber. \
                    Thier stored contact details are invalid.",
                );
            }
        }
    }
    Ok(HttpResponse::Ok().finish())
}

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}

#[derive(serde::Deserialize)]
pub struct Content {
    html: String,
    text: String,
}

struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

fn basic_authentification(headers: &HeaderMap) -> Z2PResult<Credentials> {
    // any error that happens in this fn is mapping to Error::UnexpectedError(anyhow::Error)
    // The header value, if present, must be a valid UTF8 string
    let header_value = headers
        .get("Authorization")
        .context("The `Authorization` header was missing.")
        .map_err(Error::BadRequestAuthHeader)?
        .to_str()
        .context("The `Authorization` header was not a valid UTF8 string.")
        .map_err(Error::BadRequestAuthHeader)?;
    let base64encoded_segment = header_value
        .strip_prefix("Basic ")
        .context("The authorization scheme was not `Basic`.")
        .map_err(Error::BadRequestAuthHeader)?;
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64encoded_segment)
        .context("Failed to base64-decode `Basic` credentials.")
        .map_err(Error::BadRequestAuthHeader)?;
    let decoded_credentials = String::from_utf8(decoded_bytes)
        .context("The decoded credentials string is not a valid UTF8.")
        .map_err(Error::BadRequestAuthHeader)?;
    // Split into two segments, using ':' as delimiter
    let mut credentials = decoded_credentials.splitn(2, ':');
    let username = credentials
        .next()
        .context("A username must be provided in 'Basic' auth.")
        .map_err(Error::BadRequestAuthHeader)?
        .to_string();
    let password = credentials
        .next()
        .context("A password must be provided in 'Basic' auth.")
        .map_err(Error::BadRequestAuthHeader)?
        .to_string();

    Ok(Credentials {
        username,
        password: Secret::new(password),
    })
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(
    pool: &PgPool,
) -> Z2PResult<Vec<Z2PResult<ConfirmedSubscriber>>> {
    let confirmed_subscribers = sqlx::query!(
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = $1
        "#,
        SubscriptionsStatus::Confirmed as SubscriptionsStatus,
    )
    .fetch_all(pool)
    .await
    .context("Failed to read confirmed subscribers from database.")?
    .into_iter()
    .map(|r| {
        SubscriberEmail::parse(r.email)
            .map(|email| ConfirmedSubscriber { email })
            .map_err(Error::from)
    })
    .collect();
    Ok(confirmed_subscribers)
}
