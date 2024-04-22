//! tests/api/newsletter.rs

use crate::helpers::{assert_is_redirect_to, spawn_app, ConfirmationLinks, TestApp};
use wiremock::matchers::{any, method, path};
use wiremock::{Mock, ResponseTemplate};
use zero2prod::domain::SubscriberEmail;
use zero2prod::routes::NewsletterFormData;

/// Use the public API of the application under test to create an unconfirmed subscriber
async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    let _mock_guard = Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;
    app.post_subscriptions(body.into())
        .await
        .error_for_status()
        .unwrap();

    // We inspect the requests received by the mock Postmark server
    // to retrieve the confirmation link an return it
    let email_request = &app
        .email_server
        .received_requests()
        .await
        .unwrap()
        .pop()
        .unwrap();
    app.get_confirmation_links(email_request)
}

async fn create_confirmed_subscriber(app: &TestApp) {
    // We can reuse the same helper and just add
    // an extra step to actually call the confirmation link!
    let confirmation_link = create_unconfirmed_subscriber(app).await;
    reqwest::get(confirmation_link.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

async fn make_valid_subscriber_email_invalid(app: &TestApp) {
    // get user_id from email
    let email = SubscriberEmail::parse("ursula_le_guin@gmail.com".into()).unwrap();
    let subscriber_id = sqlx::query!(
        "SELECT id FROM subscriptions \
        WHERE email = $1",
        email.as_ref(),
    )
    .fetch_one(&app.db_pool)
    .await
    .unwrap()
    .id;
    // make invalid
    let invalid_email = "ursula_le_guin-gmail.com";
    sqlx::query!(
        r#"UPDATE subscriptions SET email = $1 WHERE id = $2"#,
        invalid_email,
        subscriber_id,
    )
    .execute(&app.db_pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn you_must_be_logged_in_to_see_the_publish_newsletter_form() {
    // Arrange
    let test_app = spawn_app().await;

    // Act
    let response = test_app.get_publish_newsletter().await;

    // Assert
    assert_is_redirect_to(&response, "/login")
}

#[tokio::test]
async fn you_must_set_title_for_newsletter() {
    // Arrange
    let test_app = spawn_app().await;
    let invalid_form = NewsletterFormData {
        title: "".to_string(),
        html_content: "<p>Newsletter body as HTML</p>".to_string(),
        text_content: "Newsletter body as plain text".to_string(),
    };

    // Act - Part 1 - Login
    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password
        }))
        .await;

    // Act - Part 2 - try send invalid newsletter form
    let response = test_app.post_newsletters(invalid_form).await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains("<p><i>You must set a title for your newsletter.</i></p>"));
}

#[tokio::test]
async fn you_must_set_content_for_newsletter() {
    // Arrange
    let test_app = spawn_app().await;
    let invalid_form = NewsletterFormData {
        title: "Newsletter title".to_string(),
        html_content: "".to_string(),
        text_content: "".to_string(),
    };

    // Act - Part 1 - Login
    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password
        }))
        .await;

    // Act - Part 2 - try send invalid newsletter form
    let response = test_app.post_newsletters(invalid_form).await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains("<p><i>You must set content for your newsletter.</i></p>"));
}

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    // Arrange
    let test_app = spawn_app().await;
    create_unconfirmed_subscriber(&test_app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        // We assert that no request is fired at Postmark!
        .expect(0)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Login
    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password
        }))
        .await;

    // Act - Part 2 - try send newsletter
    let response = test_app
        .post_newsletters(NewsletterFormData {
            title: "Newsletter title".to_string(),
            html_content: "<p>Newsletter body as HTML</p>".to_string(),
            text_content: "Newsletter body as plain text".to_string(),
        })
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page
        .contains("<p><i>You have no confirmed subscribers to send your newsletter to.</i></p>"));

    // Mock verifies on Drop that we haven't sent the newsletter email
}

#[tokio::test]
async fn return_warning_if_invalid_subscriber() {
    // Arrange
    let test_app = spawn_app().await;
    create_confirmed_subscriber(&test_app).await;
    make_valid_subscriber_email_invalid(&test_app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        // We assert that no request is fired at Postmark!
        .expect(0)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Login
    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password
        }))
        .await;

    // Act - Part 2 - try send newsletter
    let response = test_app
        .post_newsletters(NewsletterFormData {
            title: "Newsletter title".to_string(),
            html_content: "<p>Newsletter body as HTML</p>".to_string(),
            text_content: "Newsletter body as plain text".to_string(),
        })
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page
        .contains("<p><i>You have at least one invalid subscriber. Check your logs.</i></p>"));

    // Mock verifies on Drop that we haven't sent the newsletter email
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    // Arrange
    let test_app = spawn_app().await;
    create_confirmed_subscriber(&test_app).await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Login
    test_app
        .post_login(&serde_json::json!({
            "username": &test_app.test_user.username,
            "password": &test_app.test_user.password
        }))
        .await;

    // Act - Part 2 -
    let response = test_app
        .post_newsletters(NewsletterFormData {
            title: "Newsletter title".to_string(),
            html_content: "<p>Newsletter body as HTML</p>".to_string(),
            text_content: "Newsletter body as plain text".to_string(),
        })
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains("<p><i>Newsletter has been sent.</i></p>"));

    // Mock verifies on Drop that we have sent one newsletter email
}
