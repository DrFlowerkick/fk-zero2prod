//! tests/api/delivery_overview.rs

use crate::helpers::{assert_is_redirect_to, spawn_app};
use crate::newsletter::{
    create_confirmed_subscriber, valid_newsletter_form_data, when_sending_an_email,
};

use wiremock::ResponseTemplate;

#[tokio::test]
async fn overview_of_delivered_newsletters_contains_newsletter_title() {
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
    let newsletter = valid_newsletter_form_data();
    let response = test_app.post_newsletters(&newsletter).await;

    // Assert
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 3 - Follow the redirect
    let html_page = test_app.get_publish_newsletter_html().await;
    assert!(html_page.contains(
        "<p><i>The newsletter issue has been accepted - \
        emails will go out shortly.</i></p>"
    ));
    test_app.dispatch_all_pending_emails().await;

    // Act - Part 4 - Overview contains newsletter title
    let html_page = test_app.get_delivery_overview_html().await;
    assert!(html_page.contains(&newsletter.title));

    // Mock verifies on Drop that we have sent one newsletter email
}
