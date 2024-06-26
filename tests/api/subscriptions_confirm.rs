//! tests/api/subscriptions_confirm.rs

use crate::helpers::{assert_is_redirect_to, spawn_app};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};
use zero2prod::domain::SubscriberToken;
use zero2prod::routes::SubscriptionsStatus;

#[tokio::test]
async fn confirmations_without_token_are_rejected_with_a_400() {
    // Arrange
    let test_app = spawn_app().await;

    // Act
    let response = reqwest::get(&format!("{}/subscriptions/confirm", test_app.address))
        .await
        .unwrap();

    // Assert
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn confirmation_link_with_empty_or_invalid_or_not_existing_token_redirects_to_subscriptions_token(
) {
    // Arrange
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    let test_tokens = [
        ("".to_owned(), "empty token"),
        (
            std::iter::repeat_with(|| '_').take(25).collect(),
            "invalid token",
        ),
        (
            std::iter::repeat_with(|| '1').take(25).collect(),
            "not existing token",
        ),
    ];

    for (test_token, test_failing_message) in test_tokens {
        let confirmation_link = format!(
            "{}/subscriptions/confirm?subscription_token={}",
            test_app.address, test_token
        );
        let confirmation_link = reqwest::Url::parse(&confirmation_link).unwrap();

        // Act - Part 1 - get confirmation link
        let response = test_app.click_email_link(confirmation_link).await;

        // Assert
        assert_is_redirect_to(&response, "/subscriptions/token");

        // Act - Part 2 - Follow the redirect
        let html_page = test_app.get_subscriptions_token_html().await;

        // Assert
        assert!(
            html_page.contains(&format!(
                "<p><i>`{}` is not a valid subscriber token.</i></p>",
                test_token
            )),
            // Additional customized error message on test failure
            "The API did not react with correct html response when payload was {}.",
            test_failing_message
        );
    }
}

#[tokio::test]
async fn the_confirmation_link_returns_a_confirmation_message_if_called_first() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscriptions(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app
        .get_email_links(&email_request)
        .html
        .confirmation
        .unwrap();

    // Act - Part 1 - get confirmation link
    let response = test_app
        .click_email_link(confirmation_link)
        .await
        .error_for_status()
        .unwrap();

    // Assert
    assert_eq!(response.url().path(), "/subscriptions/confirm");

    // Act - Part 2 - get html confirmation message
    let html_page = response.text().await.unwrap();

    assert!(html_page.contains(
        "<p><i>Welcome `le guin`. You have successfully subscribed to our newsletter!</i></p>"
    ));
}

#[tokio::test]
async fn clicking_on_the_confirmation_link_persists_a_subscriber() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscriptions(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app
        .get_email_links(&email_request)
        .html
        .confirmation
        .unwrap();

    // Act
    test_app
        .click_email_link(confirmation_link)
        .await
        .error_for_status()
        .unwrap();

    // Assert
    let saved = sqlx::query!(
        "SELECT email, name, status AS \"status: SubscriptionsStatus\" from subscriptions"
    )
    .fetch_one(&test_app.db_pool)
    .await
    .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, SubscriptionsStatus::Confirmed);
}

#[tokio::test]
async fn the_confirmation_link_returns_a_welcome_back_message_if_called_twice_or_more() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscriptions(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app
        .get_email_links(&email_request)
        .html
        .confirmation
        .unwrap();

    // Act - Part 1 - get conformation link twice
    test_app
        .click_email_link(confirmation_link.clone())
        .await
        .error_for_status()
        .unwrap();
    let response = test_app
        .click_email_link(confirmation_link)
        .await
        .error_for_status()
        .unwrap();

    // Assert
    assert_eq!(response.url().path(), "/subscriptions/confirm");

    // Act - Part 2 - get html welcome back message
    let html_page = response.text().await.unwrap();

    assert!(html_page.contains("<p><i> Welcome back `le guin`!</i></p>"));
}

#[tokio::test]
async fn subscribing_an_already_confirmed_email_redirects_directly_to_confirm_page() {
    // Arrange
    let test_app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    test_app.post_subscriptions(body.into()).await;
    let email_request = &test_app.email_server.received_requests().await.unwrap()[0];
    let confirmation_link = test_app
        .get_email_links(&email_request)
        .html
        .confirmation
        .unwrap();
    test_app
        .click_email_link(confirmation_link)
        .await
        .error_for_status()
        .unwrap();
    let status =
        sqlx::query!("SELECT status AS \"status: SubscriptionsStatus\" from subscriptions")
            .fetch_one(&test_app.db_pool)
            .await
            .expect("Failed to fetch saved subscription.")
            .status;

    let token = sqlx::query!("SELECT subscription_token from subscription_tokens")
        .fetch_one(&test_app.db_pool)
        .await
        .expect("Failed to fetch saved subscription.")
        .subscription_token;
    let token = SubscriberToken::parse(token).unwrap();

    // Act - Part 1 - post second subscription
    let second_subscription_response = test_app.post_subscriptions(body.into()).await;

    // Assert
    assert_eq!(status, SubscriptionsStatus::Confirmed);
    assert_is_redirect_to(
        &second_subscription_response,
        &format!(
            "/subscriptions/confirm?subscription_token={}",
            token.as_ref()
        ),
    );

    // Act - Part 2 - get html welcome back message
    let html_page = test_app.get_subscriptions_confirm_html(token).await;

    // Assert
    assert!(html_page.contains("<p><i> Welcome back `le guin`!</i></p>"));

    // Mock asserts on drop, that exactly one confirmation email is send
}
