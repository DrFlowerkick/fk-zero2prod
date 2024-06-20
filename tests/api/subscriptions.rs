//! tests/api/subscriptions.rs

use crate::helpers::{spawn_app, assert_is_redirect_to};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};
use zero2prod::routes::SubscriptionsStatus;

/*
#[tokio::test]
async fn subscribe_returns_a_400_when_fields_are_present_but_invalid() {
    // Arrange
    let test_app = spawn_app().await;
    let test_cases = vec![
        ("name=&email=ursula_le_guin%40gmail.com", "empty name"),
        ("name=Ursula&email=", "empty email"),
        ("name=Ursula&email=definitely-not-an-email", "invalid email"),
    ];

    for (body, description) in test_cases {
        // Act
        let response = test_app.post_subscriptions(body.into()).await;

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            "The API did not return a 400 Bad Request when the payload was {}.",
            description
        );
    }
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    // Arrange
    let test_app = spawn_app().await;
    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin%40gmail.com", "missing the name"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        // Act
        let response = test_app.post_subscriptions(invalid_body.into()).await;

        // Assert
        assert_eq!(
            400,
            response.status().as_u16(),
            // Additional customized error message on test failure
            "The API did not fail with 400 Bad Request when payload was {}.",
            error_message
        );
    }
}
*/

#[tokio::test]
async fn you_must_set_valid_user_name_to_subscribe() {
    // Arrange
    let test_app = spawn_app().await;
    // all name parsing errors result in the same error ValidationError::InvalidName(name)
    // name parsing is tested in modul
    // therefore we check here only some practical failure modes
    let test_cases = vec![
        ("name=&email=ursula_le_guin%40gmail.com", "empty name"),
        ("email=ursula_le_guin%40gmail.com", "missing the name"),
        ("", "missing both name and email"),
    ];

    for (invalid_name_body, test_failing_message) in test_cases {
        // Act - Part 1 - post subscription
        let response = test_app.post_subscriptions(invalid_name_body.into()).await;

        // Assert
        assert_is_redirect_to(&response, "/subscriptions");

        
        // Act - Part 2 - Follow the redirect
        let html_page = test_app.get_subscriptions_html().await;

        // Assert
        assert!(
            html_page.contains("<p><i>`` is not a valid subscriber name.</i></p>"),
            // Additional customized error message on test failure
            "The API did not react with correct html response when payload was {}.",
            test_failing_message
        );

    }
}

#[tokio::test]
async fn you_must_set_valid_email_to_subscribe() {
    // Arrange
    let test_app = spawn_app().await;
    // all email parsing errors result in the same error ValidationError::InvalidEmail(email)
    // email parsing is tested in modul
    // therefore we check here only some practical failure modes
    let test_cases = vec![
        ("name=le%20guin", "", "missing the email"),
        ("name=Ursula&email=", "", "empty email"),
        ("name=Ursula&email=definitely-not-an-email", "definitely-not-an-email", "invalid email"),
    ];

    for (invalid_name_body, invalid_email, test_failing_message) in test_cases {
        // Act - Part 1 - post subscription
        let response = test_app.post_subscriptions(invalid_name_body.into()).await;

        // Assert
        assert_is_redirect_to(&response, "/subscriptions");

        
        // Act - Part 2 - Follow the redirect
        let html_page = test_app.get_subscriptions_html().await;

        // Assert
        assert!(
            html_page.contains(&format!("<p><i>`{}` is not a valid subscriber email.</i></p>", invalid_email)),
            // Additional customized error message on test failure
            "The API did not react with correct html response when payload was {}.",
            test_failing_message
        );

    }
}

/*
// ToDo: rework test. Do not return 200. Instead check for Html flash message
// check newsletter tests: use assert_is_redirect_to
#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    // Act
    let response = test_app.post_subscriptions(body.into()).await;

    // Assert
    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_persists_the_new_subscriber() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    // Act
    test_app.post_subscriptions(body.into()).await;

    // Assert
    let saved = sqlx::query!(
        "SELECT email, name, status AS \"status: SubscriptionsStatus\" FROM subscriptions"
    )
    .fetch_one(&test_app.db_pool)
    .await
    .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, SubscriptionsStatus::PendingConfirmation);
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_for_valid_data() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    // Act
    test_app.post_subscriptions(body.into()).await;

    // Assert
    // Mock asserts on drop
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_with_a_link() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    // Act
    test_app.post_subscriptions(body.into()).await;

    // Assert
    // Get the first intercepted request
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_links = test_app.get_confirmation_links(&email_request);
    // The two links should be identical
    assert_eq!(confirmation_links.html, confirmation_links.plain_text);
}

#[tokio::test]
async fn subscribing_twice_sends_two_confirmation_emails_with_same_confirmation_links_and_recievers(
) {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    // Act
    let response_first = test_app.post_subscriptions(body.into()).await;
    let response_second = test_app.post_subscriptions(body.into()).await;
    let email_requests = &test_app.email_server.received_requests().await.unwrap();

    // Assert
    assert_eq!(200, response_first.status().as_u16(), "first subscription");
    assert_eq!(
        200,
        response_second.status().as_u16(),
        "second subscription"
    );
    let confirmation_links_first = test_app.get_confirmation_links(&email_requests[0]);
    let confirmation_links_second = test_app.get_confirmation_links(&email_requests[1]);
    assert_eq!(confirmation_links_first, confirmation_links_second);
    let reciever_email_first = test_app.get_reciever_email(&email_requests[0]);
    let reciever_email_second = test_app.get_reciever_email(&email_requests[1]);
    assert_eq!(
        reciever_email_first.as_ref(),
        reciever_email_second.as_ref()
    );
}
*/

#[tokio::test]
async fn subscribe_fails_if_there_is_a_fatal_database_error() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    // sabotage the database
    sqlx::query!("ALTER TABLE subscription_tokens DROP COLUMN subscription_token;",)
        .execute(&test_app.db_pool)
        .await
        .unwrap();

    // Act
    let response = test_app.post_subscriptions(body.into()).await;

    // Assert
    assert_eq!(response.status().as_u16(), 500);
}
