//! tests/api/newsletter.rs

use crate::helpers::{assert_is_redirect_to, spawn_app, ConfirmationLinks, TestApp};
use fake::{
    faker::{internet::en::SafeEmail, name::en::Name},
    Fake,
};
use std::time::Duration;
use wiremock::{
    matchers::{any, method, path},
    Mock, MockBuilder, ResponseTemplate,
};
use zero2prod::domain::{SubscriberEmail, SubscriberName};
use zero2prod::idempotency::delete_outlived_idempotency_key;
use zero2prod::routes::NewsletterFormData;

/// have some helpers for Newsletters
pub fn valid_newsletter_form_data() -> NewsletterFormData {
    NewsletterFormData {
        title: "Newsletter title".to_string(),
        html_content: "<p>Newsletter body as HTML</p>".to_string(),
        text_content: "Newsletter body as plain text".to_string(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
    }
}

fn invalid_title_newsletter_form_data() -> NewsletterFormData {
    NewsletterFormData {
        title: "".to_string(),
        html_content: "<p>Newsletter body as HTML</p>".to_string(),
        text_content: "Newsletter body as plain text".to_string(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
    }
}

fn invalid_text_content_newsletter_form_data() -> NewsletterFormData {
    NewsletterFormData {
        title: "Newsletter title".to_string(),
        html_content: "<p>Newsletter body as HTML</p>".to_string(),
        text_content: "".to_string(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
    }
}

fn invalid_html_content_newsletter_form_data() -> NewsletterFormData {
    NewsletterFormData {
        title: "Newsletter title".to_string(),
        html_content: "".to_string(),
        text_content: "Newsletter body as plain text".to_string(),
        idempotency_key: uuid::Uuid::new_v4().to_string(),
    }
}

// Short-hand for a common mocking setup
pub fn when_sending_an_email() -> MockBuilder {
    Mock::given(path("/email")).and(method("POST"))
}

/// Use the public API of the application under test to create an unconfirmed subscriber
async fn create_unconfirmed_subscriber(app: &TestApp) -> (SubscriberEmail, SubscriberName, ConfirmationLinks) {
    // We support working with multiple subscribers,
    // thier details must be randomized to avoid conflicts.
    let name: String = Name().fake();
    let email: String = SafeEmail().fake();
    let body = serde_urlencoded::to_string(&serde_json::json!({
        "name": name,
        "email": email
    }))
    .unwrap();

    let email = SubscriberEmail::parse(email).unwrap();
    let name = SubscriberName::parse(name).unwrap();

    let _mock_guard = when_sending_an_email()
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
    (email, name, app.get_confirmation_links(email_request))
}

pub async fn create_confirmed_subscriber(app: &TestApp) -> (SubscriberEmail, SubscriberName) {
    // We can reuse the same helper and just add
    // an extra step to actually call the confirmation link!
    let (email, name, confirmation_link) = create_unconfirmed_subscriber(app).await;
    reqwest::get(confirmation_link.html)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    (email, name)
}

async fn make_valid_subscriber_email_invalid(app: &TestApp, email: SubscriberEmail) {
    // get user_id from email
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
    let invalid_email = email.as_ref().replace("@", "_at_");
    sqlx::query!(
        r#"UPDATE subscriptions SET email = $1 WHERE id = $2"#,
        invalid_email,
        subscriber_id,
    )
    .execute(&app.db_pool)
    .await
    .unwrap();
}

async fn make_valid_subscriber_name_invalid(app: &TestApp, name: SubscriberName) {
    // get user_id from email
    let subscriber_id = sqlx::query!(
        "SELECT id FROM subscriptions \
        WHERE name = $1",
        name.as_ref(),
    )
    .fetch_one(&app.db_pool)
    .await
    .unwrap()
    .id;
    // make invalid by appending 
    let invalid_name = name.as_ref().to_owned() + "()<>";
    sqlx::query!(
        r#"UPDATE subscriptions SET name = $1 WHERE id = $2"#,
        invalid_name,
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
    let invalid_form = invalid_title_newsletter_form_data();

    // Act - Part 1 - Login
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 - try send invalid newsletter form
    let response = test_app.post_newsletters(&invalid_form).await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains("<p><i>You must set a title for your newsletter.</i></p>"));
}

#[tokio::test]
async fn you_must_set_text_content_for_newsletter() {
    // Arrange
    let test_app = spawn_app().await;
    let invalid_form = invalid_text_content_newsletter_form_data();

    // Act - Part 1 - Login
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 - try send invalid newsletter form
    let response = test_app.post_newsletters(&invalid_form).await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains("<p><i>You must set text content for your newsletter.</i></p>"));
}

#[tokio::test]
async fn you_must_set_html_content_for_newsletter() {
    // Arrange
    let test_app = spawn_app().await;
    let invalid_form = invalid_html_content_newsletter_form_data();

    // Act - Part 1 - Login
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 - try send invalid newsletter form
    let response = test_app.post_newsletters(&invalid_form).await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains("<p><i>You must set html content for your newsletter.</i></p>"));
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
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 - try send newsletter
    let response = test_app
        .post_newsletters(&valid_newsletter_form_data())
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));
    test_app.dispatch_all_pending_emails().await;
    // Mock verifies on Drop that we haven't sent the newsletter email
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers_only() {
    // Arrange
    let test_app = spawn_app().await;
    create_confirmed_subscriber(&test_app).await;

    when_sending_an_email()
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Login
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 -
    let response = test_app
        .post_newsletters(&valid_newsletter_form_data())
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));
    test_app.dispatch_all_pending_emails().await;

    // Assert for two subscribers, one being invalid
    let newsletter_delivery_overview = test_app.get_newsletter_delivery_overview().await;
    assert_eq!(
        newsletter_delivery_overview.num_current_subscribers,
        Some(1)
    );
    assert_eq!(
        newsletter_delivery_overview.num_delivered_newsletters,
        Some(1)
    );
    assert_eq!(newsletter_delivery_overview.num_failed_deliveries, Some(0));

    // Mock verifies on Drop that we have sent one newsletter email
}

#[tokio::test]
async fn invalid_emails_are_discarded_from_task_pool() {
    // Arrange
    let test_app = spawn_app().await;
    let invalid_email = create_confirmed_subscriber(&test_app).await.0;
    make_valid_subscriber_email_invalid(&test_app, invalid_email).await;

    when_sending_an_email()
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Login
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 -
    let response = test_app
        .post_newsletters(&valid_newsletter_form_data())
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));
    test_app.dispatch_all_pending_emails().await;

    // Assert for two subscribers, one being invalid
    let newsletter_delivery_overview = test_app.get_newsletter_delivery_overview().await;
    assert_eq!(
        newsletter_delivery_overview.num_current_subscribers,
        Some(1)
    );
    assert_eq!(
        newsletter_delivery_overview.num_delivered_newsletters,
        Some(0)
    );
    assert_eq!(newsletter_delivery_overview.num_failed_deliveries, Some(1));

    // Mock verifies on Drop that we have sent no newsletter email
}

#[tokio::test]
async fn invalid_names_are_discarded_from_task_pool() {
    // Arrange
    let test_app = spawn_app().await;
    let invalid_name = create_confirmed_subscriber(&test_app).await.1;
    make_valid_subscriber_name_invalid(&test_app, invalid_name).await;

    when_sending_an_email()
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Login
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 -
    let response = test_app
        .post_newsletters(&valid_newsletter_form_data())
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));
    test_app.dispatch_all_pending_emails().await;

    // Assert for two subscribers, one being invalid
    let newsletter_delivery_overview = test_app.get_newsletter_delivery_overview().await;
    assert_eq!(
        newsletter_delivery_overview.num_current_subscribers,
        Some(1)
    );
    assert_eq!(
        newsletter_delivery_overview.num_delivered_newsletters,
        Some(0)
    );
    assert_eq!(newsletter_delivery_overview.num_failed_deliveries, Some(1));

    // Mock verifies on Drop that we have sent no newsletter email
}

#[tokio::test]
async fn sending_of_newsletter_is_retried_n_times_at_transient_failures() {
    // Arrange
    let test_app = spawn_app().await;
    create_confirmed_subscriber(&test_app).await;

    // add +1 to n_retries since it is one first try and than n retries.
    when_sending_an_email()
        .respond_with(ResponseTemplate::new(500))
        .expect((test_app.n_retries + 1) as u64)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Login
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 -
    let response = test_app
        .post_newsletters(&valid_newsletter_form_data())
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));

    // Assert postponed tasks
    assert!(test_app.dispatch_all_pending_emails().await);

    // Assert delivery details
    let newsletter_delivery_overview = test_app.get_newsletter_delivery_overview().await;
    assert_eq!(
        newsletter_delivery_overview.num_current_subscribers,
        Some(1)
    );
    assert_eq!(
        newsletter_delivery_overview.num_delivered_newsletters,
        Some(0)
    );
    assert_eq!(newsletter_delivery_overview.num_failed_deliveries, Some(1));

    // Mock verifies on Drop that we have tried to send n newsletter email
}

#[tokio::test]
async fn newsletter_is_send_after_a_transient_failure() {
    // Arrange
    let test_app = spawn_app().await;
    create_confirmed_subscriber(&test_app).await;

    // first email is answered with failure
    // second email should pass
    when_sending_an_email()
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&test_app.email_server)
        .await;
    when_sending_an_email()
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Login
    test_app.test_user.login(&test_app).await;

    // Act - Part 2 -
    let response = test_app
        .post_newsletters(&valid_newsletter_form_data())
        .await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));

    // Assert postponed tasks
    assert!(test_app.dispatch_all_pending_emails().await);

    // Assert for delivery details
    let newsletter_delivery_overview = test_app.get_newsletter_delivery_overview().await;
    assert_eq!(
        newsletter_delivery_overview.num_current_subscribers,
        Some(1)
    );
    assert_eq!(
        newsletter_delivery_overview.num_delivered_newsletters,
        Some(1)
    );
    assert_eq!(newsletter_delivery_overview.num_failed_deliveries, Some(0));

    // Mock verifies on Drop that we have tried to send one email (answered with failure)
    // and sent one newsletter email
}

#[tokio::test]
async fn newsletter_creation_is_idempotent() {
    // Arrange
    let test_app = spawn_app().await;
    create_confirmed_subscriber(&test_app).await;
    test_app.test_user.login(&test_app).await;

    when_sending_an_email()
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    // Act - Part 1 - Submit newsletter form
    let newsletter_request_body = valid_newsletter_form_data();
    let response = test_app.post_newsletters(&newsletter_request_body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 2 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));

    // Act - Part 3 - Submit newsletter form **again**
    let response = test_app.post_newsletters(&newsletter_request_body).await;
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 4 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));
    test_app.dispatch_all_pending_emails().await;

    // Act - Part 5 - delete 1 idempotency key
    assert_eq!(
        delete_outlived_idempotency_key(&test_app.db_pool, 0)
            .await
            .unwrap(),
        1
    );

    // Mock verifies on Drop that we have sent the newsletter email **once**
}

#[tokio::test]
async fn concurrent_form_submission_is_handled_gracefully() {
    // Arrange
    let test_app = spawn_app().await;
    create_confirmed_subscriber(&test_app).await;
    test_app.test_user.login(&test_app).await;

    when_sending_an_email()
        // Setting a long delay to ensure that the second request
        // arrives before the first one completes
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)))
        .expect(1)
        .mount(&test_app.email_server)
        .await;

    // Act - Submit two newsletter forms concurrently
    let newsletter_request_body = valid_newsletter_form_data();
    let response1 = test_app.post_newsletters(&newsletter_request_body);
    let response2 = test_app.post_newsletters(&newsletter_request_body);
    let (response1, response2) = tokio::join!(response1, response2);

    // Assert
    assert_eq!(response1.status(), response2.status());
    assert_eq!(
        response1.text().await.unwrap(),
        response2.text().await.unwrap()
    );
    test_app.dispatch_all_pending_emails().await;
    // Mock verifies on Drop that we have sent the newsletter email **once**
}
