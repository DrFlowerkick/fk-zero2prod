//! tests/api/helpers.rs

use anyhow::Error;
use argon2::password_hash::SaltString;
use argon2::{Algorithm, Argon2, Params, PasswordHasher, Version};
use async_once_cell::OnceCell;
use lazy_static::lazy_static;
use once_cell::sync::Lazy;
use reqwest::Url;
use scraper::{Html, Selector};
use sqlx::{Connection, Executor, PgConnection, PgPool, Row};
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;
use wiremock::MockServer;
use zero2prod::configuration::{get_configuration, DatabaseSettings};
use zero2prod::domain::SubscriberEmail;
use zero2prod::email_client::EmailClient;
use zero2prod::issue_delivery_worker::{try_execute_task, ExecutionOutcome};
use zero2prod::routes::NewsletterFormData;
use zero2prod::startup::{get_connection_pool, Application};
use zero2prod::telemetry::{get_subscriber, init_subscriber};

static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();
    // We cannot assign the output of `get_subscriber` to a variable based on the
    // value TEST_LOG` because the sink is part of the type returned by
    // `get_subscriber`, therefore they are not the same type. We could work around
    // it, but this is the most straight-forward way of moving forward.
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);
    }
});

lazy_static! {
    static ref CLEANUP_DB: OnceCell<Result<(), Error>> = OnceCell::new();
}

pub struct TestUser {
    pub user_id: Uuid,
    pub username: String,
    pub password: String,
}

impl TestUser {
    pub fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
        }
    }

    async fn store(&self, pool: &PgPool) {
        let salt = SaltString::generate(&mut rand::thread_rng());
        // We don't care about the exact Argon2 parameters here
        // given that it's for testing purposes!
        let password_hash = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(15_000, 2, 1, None).unwrap(),
        )
        .hash_password(self.password.as_bytes(), &salt)
        .unwrap()
        .to_string();
        sqlx::query!(
            "INSERT INTO users (user_id, username, password_hash)
            VALUES ($1, $2, $3)",
            self.user_id,
            self.username,
            password_hash,
        )
        .execute(pool)
        .await
        .expect("Failed to create test user.");
    }
    pub async fn login(&self, app: &TestApp) -> reqwest::Response {
        app.post_login(&serde_json::json!({
            "username": &self.username,
            "password": &self.password
        }))
        .await
    }
}

pub struct NewsletterDeliveryOverview {
    pub num_current_subscribers: Option<i32>,
    pub num_delivered_newsletters: Option<i32>,
    pub num_failed_deliveries: Option<i32>,
}

pub struct TestApp {
    pub address: String,
    pub port: u16,
    pub db_pool: PgPool,
    pub email_server: MockServer,
    pub test_user: TestUser,
    pub api_client: reqwest::Client,
    pub email_client: EmailClient,
    pub db_name: String,
    pub n_retries: u8,
    pub time_delta: chrono::TimeDelta,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        self.api_client
            .post(&format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    /// Extract the confirmation links embedded in the request to the email API.
    pub fn get_confirmation_links(&self, email_request: &wiremock::Request) -> ConfirmationLinks {
        // Parse the body as JSON, starting from raw bytes
        let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();
        // Extract the link from one of the request fields.
        let get_link = |s: &str| {
            let links: Vec<_> = linkify::LinkFinder::new()
                .links(s)
                .filter(|l| *l.kind() == linkify::LinkKind::Url)
                .collect();
            assert_eq!(links.len(), 1);
            let raw_link = links[0].as_str().to_owned();
            let mut confirmation_link = reqwest::Url::parse(&raw_link).unwrap();
            // Let's make sure we don't call random APIs on the web
            assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");
            // Let's rewrite the URL to include the port
            confirmation_link.set_port(Some(self.port)).unwrap();
            confirmation_link
        };

        let html = get_link(&body["HtmlBody"].as_str().unwrap());
        let plain_text = get_link(&body["TextBody"].as_str().unwrap());
        ConfirmationLinks { html, plain_text }
    }

    /// Extract the reciever email from the request to the email API.
    pub fn get_reciever_email(&self, email_request: &wiremock::Request) -> SubscriberEmail {
        // Parse the body as JSON, starting from raw bytes
        let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();
        // get reciever from body
        let reciever_email = body["To"].as_str().unwrap();
        let reciever_email = SubscriberEmail::parse(reciever_email.to_owned()).unwrap();
        reciever_email
    }

    /// Post newsletters
    pub async fn post_newsletters(&self, form: &NewsletterFormData) -> reqwest::Response {
        self.api_client
            .post(&format!("{}/admin/newsletters", &self.address))
            .form(form)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    /// helper for sending a POST /login request
    pub async fn post_login<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(&format!("{}/login", &self.address))
            // This 'reqwest' method makes sure that the body is URL-encoded
            // and the 'Content-Type' header is set accordingly.
            .form(body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    /// helper to get Response from url
    pub async fn get_response_from_url(&self, path: &str) -> reqwest::Response {
        self.api_client
            .get(&format!("{}{}", self.address, path))
            .send()
            .await
            .expect("Failed to execute request.")
    }

    /// helper to get login html
    // Out tests will only look at the HTML page, therefore
    // we do not expose the underlying reqwest::Response
    pub async fn get_login_html(&self) -> String {
        self.get_response_from_url("/login")
            .await
            .text()
            .await
            .unwrap()
    }

    /// helper to get subscriptions response
    pub async fn get_subscriptions(&self) -> reqwest::Response {
        self.get_response_from_url("/subscriptions").await
    }

    /// helper to get subscriptions html
    pub async fn get_subscriptions_html(&self) -> String {
        self.get_subscriptions().await.text().await.unwrap()
    }

    /// helper to get subscriptions/confirm response
    pub async fn get_subscriptions_confirm(&self) -> reqwest::Response {
        self.get_response_from_url("/subscriptions/confirm").await
    }

    /// helper to get subscriptions/confirm html
    pub async fn get_subscriptions_confirm_html(&self) -> String {
        self.get_subscriptions_confirm().await.text().await.unwrap()
    }

    /// helper to get admin dashboard
    pub async fn get_admin_dashboard(&self) -> reqwest::Response {
        self.get_response_from_url("/admin/dashboard").await
    }

    /// helper to get admin dashboard html
    pub async fn get_admin_dashboard_html(&self) -> String {
        self.get_admin_dashboard().await.text().await.unwrap()
    }

    /// helper to get publish newsletter
    pub async fn get_publish_newsletter(&self) -> reqwest::Response {
        self.get_response_from_url("/admin/newsletters").await
    }

    /// helper to get publish newsletter html
    pub async fn get_publish_newsletter_html(&self) -> String {
        self.get_publish_newsletter().await.text().await.unwrap()
    }

    /// helper to get admin change password
    pub async fn get_change_password(&self) -> reqwest::Response {
        self.get_response_from_url("/admin/password").await
    }

    /// helper to get admin dashboard html
    pub async fn get_change_password_html(&self) -> String {
        self.get_change_password().await.text().await.unwrap()
    }

    /// helper to change admin password
    pub async fn post_change_password<Body>(&self, body: &Body) -> reqwest::Response
    where
        Body: serde::Serialize,
    {
        self.api_client
            .post(&format!("{}/admin/password", self.address))
            .form(body)
            .send()
            .await
            .expect("Failed to execute request.")
    }

    /// helper to log out
    pub async fn post_logout(&self) -> reqwest::Response {
        self.api_client
            .post(&format!("{}/admin/logout", self.address))
            .send()
            .await
            .expect("Failed to execute request.")
    }

    /// helper to send all newsletter emails from task queue
    pub async fn dispatch_all_pending_emails(&self) -> bool {
        let mut postponed_tasks = false;
        loop {
            match try_execute_task(
                &self.db_pool,
                &self.email_client,
                self.n_retries,
                self.time_delta,
            )
            .await
            .unwrap()
            {
                ExecutionOutcome::EmptyQueue => break,
                ExecutionOutcome::PostponedTasks => {
                    postponed_tasks = true;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                ExecutionOutcome::TaskCompleted => {}
            }
        }
        postponed_tasks
    }

    /// helper to read newsletter delivery overview
    pub async fn get_newsletter_delivery_overview(&self) -> NewsletterDeliveryOverview {
        sqlx::query_as!(
            NewsletterDeliveryOverview,
            r#"
            SELECT num_current_subscribers, num_delivered_newsletters, num_failed_deliveries
            FROM newsletter_issues
            "#
        )
        .fetch_one(&self.db_pool)
        .await
        .unwrap()
    }

    /// helper to get delivery overview html
    pub async fn get_delivery_overview_html(&self) -> String {
        //self.get_response_from_url("/admin/delivery_overview")
        self.api_client
            .get(&format!("{}/admin/delivery_overview", self.address))
            .send()
            .await
            .expect("Failed to execute request.")
            //.await
            .text()
            .await
            .unwrap()
    }

    /// helper to extract newsletter issue html
    pub async fn get_delivered_newsletter_issue_id_html(&self) -> String {
        let html = self.get_delivery_overview_html().await;
        let base_url = Url::parse(&self.address).unwrap();
        // Parse the HTML content
        let document = Html::parse_document(&html);
        // Create a selector for <a> tags with the ID "issue"
        let selector = Selector::parse("a#issue").unwrap();

        let mut link: Option<Url> = None;
        for element in document.select(&selector) {
            if let Some(href) = element.value().attr("href") {
                let absolute_url = match Url::parse(href) {
                    Ok(url) => url,
                    Err(_) => base_url.join(href).unwrap(),
                };

                link = Some(absolute_url);
                break;
            }
        }
        assert!(link.is_some());
        let mut issue_id_link = link.unwrap();
        // Let's make sure we don't call random APIs on the web
        assert_eq!(issue_id_link.host_str().unwrap(), "127.0.0.1");
        // Let's rewrite the URL to include the port
        issue_id_link.set_port(Some(self.port)).unwrap();
        // Check that link ends on a valid Uuid
        let query = issue_id_link.query().unwrap();
        let uuid = query.split_once('=').unwrap().1;
        assert!(Uuid::from_str(uuid).is_ok());
        // get html of link
        self.api_client
            .get(issue_id_link)
            .send()
            .await
            .expect("Failed to execute request.")
            .text()
            .await
            .unwrap()
    }
}

// Little helper function to assert redirected location
pub fn assert_is_redirect_to(response: &reqwest::Response, location: &str) {
    assert_eq!(response.status().as_u16(), 303);
    assert_eq!(response.headers().get("Location").unwrap(), location);
}

/// Spin up an instance of our application
/// and returns its address (i.e. http://localhost:XXXX)
pub async fn spawn_app() -> TestApp {
    // The first time `initialize` is invoked the code in `TRACING` is executed.
    // All other invocations will instead skip execution.
    Lazy::force(&TRACING);
    if let Err(r) = CLEANUP_DB.get_or_init(cleanup_db()).await {
        panic!("clean up of test databases failed:\n{}", r);
    }

    // Launch a mock server to stand in for Postmark's API
    let email_server = MockServer::start().await;

    // Randomise configuration to ensure test isolation
    let configuration = {
        let mut c = get_configuration().expect("Failed to read configuration.");
        // use different database for each test case
        c.database.database_name = Uuid::new_v4().to_string();
        // use a random OS port
        c.application.port = 0;
        // use the mock server as email API
        c.emailclient.base_url = email_server.uri();
        // reduce n_retries to shorten test time
        c.emailclient.n_retries = 3;
        // reduce execute_retry_after_milliseconds to 1000ms to shorten test time
        c.emailclient.execute_retry_after_milliseconds = 1000;
        c
    };

    // Create and migrate the database
    configure_database(&configuration.database).await;

    let application = Application::build(configuration.clone())
        .await
        .expect("Failed to build application");
    let application_port = application.port();
    let _ = tokio::spawn(application.run_until_stopped());

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(true)
        .build()
        .unwrap();

    let time_delta = chrono::TimeDelta::milliseconds(
        configuration.emailclient.execute_retry_after_milliseconds as i64,
    );

    let test_app = TestApp {
        address: format!("http://127.0.0.1:{}", application_port),
        port: application_port,
        db_pool: get_connection_pool(&configuration.database),
        email_server,
        test_user: TestUser::generate(),
        api_client: client,
        n_retries: configuration.emailclient.n_retries,
        email_client: configuration.emailclient.client(),
        db_name: configuration.database.database_name,
        time_delta,
    };
    test_app.test_user.store(&test_app.db_pool).await;
    test_app
}

async fn configure_database(config: &DatabaseSettings) -> PgPool {
    // Create database
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    // Migrate database
    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Psotgres.");
    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database.");

    connection_pool
}

async fn cleanup_db() -> Result<(), Error> {
    let database = get_configuration()?.database;
    // Connect to postgres without db
    let mut connection = PgConnection::connect_with(&database.without_db()).await?;

    let rows = connection
        .fetch_all("SELECT datname FROM pg_database WHERE datistemplate = false")
        .await?;

    for row in rows {
        let database_name: String = row.try_get("datname")?;
        if Uuid::parse_str(&database_name).is_ok() {
            // database is Uuid -> test database -> delete it
            let query: &str = &format!(r#"DROP DATABASE IF EXISTS "{}" ( FORCE ) "#, database_name);
            connection.execute(query).await?;
        }
    }
    Ok(())
}

/// Confirmation links embedded in the rquest to the email API.
#[derive(PartialEq, Eq, Debug)]
pub struct ConfirmationLinks {
    pub html: reqwest::Url,
    pub plain_text: reqwest::Url,
}
