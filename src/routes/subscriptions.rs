//! src/routes/subscriptions.rs

use actix_web::{web, HttpResponse};
use chrono::Utc;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction, Executor};
use uuid::Uuid;

use crate::domain::{NewSubscriber, SubscriberEmail, SubscriberName};
use crate::email_client::EmailClient;
use crate::startup::ApplicationBaseUrl;
use crate::routes::SubscriptionsStatus;

/// Generate a random 25-characters-long case-sensitive subscription token.
fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;
        Ok(Self { email, name })
    }
}

#[tracing::instrument(
    name = "Adding a new subscriber.",
    skip(form, pool, email_client, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> HttpResponse {
    let new_subscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest().finish(),
    };
    let subscription_token = match subscribe_transaction(&new_subscriber, pool.as_ref()).await {
        Ok(new_subscription_token) => new_subscription_token,
        Err(err) => match fetch_token_if_unique_violation(&new_subscriber, pool.as_ref(), err).await {
            Ok(existing_subscription_token) => existing_subscription_token,
            Err(_) => return HttpResponse::InternalServerError().finish(),
        },
    };
    if send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await
    .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }
    HttpResponse::Ok().finish()
}

#[tracing::instrument(
    name = "Executing the transaction to insert a new subscriber in the database.",
    skip(new_subscriber, pool)
)]
pub async fn subscribe_transaction(
    new_subscriber: &NewSubscriber,
    pool: &PgPool,
) -> Result<String, sqlx::Error> {
    // init transaction
    let mut transaction = pool.begin().await?;
    // insert subscriber in transaction
    let subscriber_id = insert_subscriber(&mut transaction, &new_subscriber).await?;
    // insert token in transaction
    let subscription_token = generate_subscription_token();
    store_token(&mut transaction, subscriber_id, &subscription_token).await?;
    // commit transaction
    transaction.commit().await?;
    // return transaction token
    Ok(subscription_token)
}

#[tracing::instrument(
    name = "Fetching subscription token from the database if unique violation.",
    skip(new_subscriber, pool, err)
)]
pub async fn fetch_token_if_unique_violation(
    new_subscriber: &NewSubscriber,
    pool: &PgPool,
    err: sqlx::Error
) -> Result<String, sqlx::Error> {
    if let Some(db_err) = err.as_database_error() {
        if !db_err.is_unique_violation() {
            return Err(err);
        }
    } else {
        return Err(err);
    }
    // get uuid from substriction table with email
    let new_subscriber_id = get_subscriber_id_from_email(pool, new_subscriber).await?;
    // 2. get subscription token with uuid
    let subscription_token = get_token_from_subscriber_id(pool, new_subscriber_id).await?;
    Ok(subscription_token)
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database.",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    let query = sqlx::query!(
        r#"INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, $5)"#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now(),
        SubscriptionsStatus::PendingConfirmation as SubscriptionsStatus,
    );
    transaction
        .execute(query)
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute query: {:?}", e);
            e
        })?;
    Ok(subscriber_id)
}

#[tracing::instrument(
    name = "Store subscription token in the database.",
    skip(subscription_token, transaction)
)]
pub async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), sqlx::Error> {
    let query = sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    );
    transaction
        .execute(query)
        .await
        .map_err(|e| {
            tracing::error!("Failed to execute query: {:?}", e);
            e
        })?;
    Ok(())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber, base_url, subscription_token)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    // We create a (useless) confirmation link
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    let plain_body = format!(
        "Welcome to our newsletter!\n
        Visit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!(
        "Welcome to our newsletter!<br />\
        Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );
    email_client
        .send_email(new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}

#[tracing::instrument(name = "Get subscriber id from email", skip(new_subscriber, pool))]
pub async fn get_subscriber_id_from_email(
    pool: &PgPool,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT id FROM subscriptions \
        WHERE email = $1",
        new_subscriber.email.as_ref(),
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(result.id)
}

#[tracing::instrument(name = "Get token from subscriber_id", skip(subscriber_id, pool))]
pub async fn get_token_from_subscriber_id(
    pool: &PgPool,
    subscriber_id: Uuid,
) -> Result<String, sqlx::Error> {
    let result = sqlx::query!(
        "SELECT subscription_token FROM subscription_tokens \
        WHERE subscriber_id = $1",
        subscriber_id,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(result.subscription_token)
}