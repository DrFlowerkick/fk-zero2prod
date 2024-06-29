//! tests/api/subscriptions_confirm.rs

use crate::helpers::{assert_is_redirect_to, spawn_app};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn unsubscribing_without_token_are_rejected_with_a_400() {
    // Arrange
    let test_app = spawn_app().await;

    // Act
    let response = reqwest::get(&format!("{}/subscriptions/unsubscribe", test_app.address))
        .await
        .unwrap();

    // Assert
    assert_eq!(response.status().as_u16(), 400);
}

#[tokio::test]
async fn unsubscribing_with_empty_or_invalid_or_not_existing_token_redirects_to_subscriptions() {
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
        let unsubscription_link = format!(
            "{}/subscriptions/unsubscribe?subscription_token={}",
            test_app.address, test_token
        );
        let unsubscription_link = reqwest::Url::parse(&unsubscription_link).unwrap();

        // Act - Part 1 - get unsubscribe link
        let response = test_app.click_email_link(unsubscription_link).await;

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
async fn unsubscribing_returns_a_confirmation_message() {
    // Arrange
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    let unsubscribe_link = test_app.subscribe_and_confirm_a_user().await;

    // Act - Part 1 - get unsubscribe link
    let response = test_app
        .click_email_link(unsubscribe_link)
        .await
        .error_for_status();
    let response = response.unwrap();

    // Assert
    assert_eq!(response.url().path(), "/subscriptions/unsubscribe");

    // Act - Part 2 - get html confirmation message
    let html_page = response.text().await.unwrap();

    // Assert returns confirmation message of unsubscribe
    assert!(html_page.contains("<p><i>Good bye `le guin`!</i></p>"));
    assert!(html_page.contains(
        "<p>You successfilly unsubscribed <a href=\"mailto:ursula_le_guin@gmail.com\">ursula_le_guin@gmail.com</a>.</p>"
    ));
}

#[tokio::test]
async fn clicking_on_the_unsubscribe_link_removes_subscriber_from_db() {
    // Arrange
    let test_app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - subscribe and confirm a user
    let unsubscribe_link = test_app.subscribe_and_confirm_a_user().await;

    // Assert tables contain one row of subscribed user
    assert_eq!(test_app.num_rows_of_table("subscriptions").await, 1);
    assert_eq!(test_app.num_rows_of_table("subscription_tokens").await, 1);

    // Act - Part 2 - unsubscribe user
    test_app
        .click_email_link(unsubscribe_link)
        .await
        .error_for_status()
        .unwrap();

    // Assert tables contain no row of subscribed user
    assert_eq!(test_app.num_rows_of_table("subscriptions").await, 0);
    assert_eq!(test_app.num_rows_of_table("subscription_tokens").await, 0);
}
