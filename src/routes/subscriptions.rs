//! src/routes/subscriptions.rs

use actix_web::{web, HttpResponse};
use chrono::Utc;
use sqlx::postgres::PgDatabaseError;
use sqlx::{Executor, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::{NewSubscriber, NewSubscriberError, SubscriberEmail, SubscriberName, SubscriberToken};
use crate::email_client::EmailClient;
use crate::routes::SubscriptionsStatus;
use crate::startup::ApplicationBaseUrl;

/// Checks if sqlx:Error results from trying to subscribe the same email twice
fn is_email_subscribed_twice_err(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        if let Some(pg_err) = db_err.try_downcast_ref::<PgDatabaseError>() {
            if db_err.is_unique_violation() {
                if let Some(table) = pg_err.table() {
                    if table == "subscriptions" {
                        if let Some(constraint) = pg_err.constraint() {
                            if constraint == "subscriptions_email_key" {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

#[derive(serde::Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = NewSubscriberError;

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
        Err(err) => {
            if is_email_subscribed_twice_err(&err) {
                // get id from new_subscriber
                let subscriber_id =
                    match get_subscriber_id_from_email(pool.as_ref(), &new_subscriber).await {
                        Ok(subscriber_id) => subscriber_id,
                        Err(_) => return HttpResponse::InternalServerError().finish(),
                    };
                // existing subscriber, check if status is confirmed
                match get_status_from_subscriber_id(pool.as_ref(), subscriber_id).await {
                    Ok(status) => {
                        if status == SubscriptionsStatus::Confirmed {
                            // new subscriber is already confirmed
                            return HttpResponse::Ok().finish();
                        }
                    }
                    Err(_) => return HttpResponse::InternalServerError().finish(),
                }
                // grab token of existing subscriber
                match fetch_token_of_subscriber(&new_subscriber, pool.as_ref()).await {
                    Ok(existing_subscription_token) => existing_subscription_token,
                    Err(_) => return HttpResponse::InternalServerError().finish(),
                }
            } else {
                return HttpResponse::InternalServerError().finish();
            }
        }
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
) -> Result<SubscriberToken, sqlx::Error> {
    // init transaction
    let mut transaction = pool.begin().await?;
    // insert subscriber in transaction
    let subscriber_id = insert_subscriber(&mut transaction, new_subscriber).await?;
    // insert token in transaction
    let subscription_token = SubscriberToken::generate_subscription_token();
    store_token(&mut transaction, subscriber_id, &subscription_token).await?;
    // commit transaction
    transaction.commit().await?;
    // return transaction token
    Ok(subscription_token)
}

#[tracing::instrument(
    name = "Fetching subscription token of subscriber from the database.",
    skip(subscriber, pool)
)]
pub async fn fetch_token_of_subscriber(
    subscriber: &NewSubscriber,
    pool: &PgPool,
) -> Result<SubscriberToken, sqlx::Error> {
    // get uuid from subscription table with email
    let subscriber_id = get_subscriber_id_from_email(pool, subscriber).await?;
    // 2. get subscription token with uuid
    let subscription_token = get_token_from_subscriber_id(pool, subscriber_id).await?;
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
    transaction.execute(query).await.map_err(|e| {
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
    subscription_token: &SubscriberToken,
) -> Result<(), sqlx::Error> {
    let query = sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token.as_ref(),
        subscriber_id
    );
    transaction.execute(query).await.map_err(|e| {
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
    subscription_token: &SubscriberToken,
) -> Result<(), reqwest::Error> {
    // We create a (useless) confirmation link
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token.as_ref()
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
) -> Result<SubscriberToken, sqlx::Error> {
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
    // ToDo: implement error handling to accept error from SubscriberToken::parse
    let subscription_token = SubscriberToken::parse(result.subscription_token).unwrap();
    Ok(subscription_token)
}

#[tracing::instrument(name = "Get status from subscriber_id", skip(subscriber_id, pool))]
pub async fn get_status_from_subscriber_id(
    pool: &PgPool,
    subscriber_id: Uuid,
) -> Result<SubscriptionsStatus, sqlx::Error> {
    // get status of entry with subscriber_id
    let result = sqlx::query!(
        "SELECT status AS \"status: SubscriptionsStatus\" FROM subscriptions \
        WHERE id = $1",
        subscriber_id,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(result.status)
}
