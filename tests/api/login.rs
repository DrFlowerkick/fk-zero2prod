//! tests/api/login.rs

use crate::helpers::{assert_is_redirect_to, spawn_app};
use reqwest::header::HeaderValue;
use std::collections::HashSet;

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    // Arrange
    let test_app = spawn_app().await;

    // Act
    let login_body = serde_json::json!({
        "username": "random-username",
        "password": "random-password"
    });
    let response = test_app.post_login(&login_body).await;

    // Assert
    assert_is_redirect_to(&response, "/login");

    // long version without cookie feature of reqwest
    let cookies: HashSet<_> = response
        .headers()
        .get_all("Set-Cookie")
        .into_iter()
        .collect();
    assert!(cookies.contains(&HeaderValue::from_str("_flash=Failed Login Authentication").unwrap()));
    // short version using cookie feature of reqwest
    let flash_cookie = response.cookies().find(|c| c.name() == "_flash").unwrap();
    assert_eq!(flash_cookie.value(), "Failed Login Authentication");

    // Act - Part 2
    let html_page = test_app.get_login_html().await;
    assert!(html_page.contains(r#"<p><i>Failed Login Authentication</i></p>"#));

    // Act - Part 3
    let html_page = test_app.get_login_html().await;
    assert!(!html_page.contains(r#"<p><i>Failed Login Authentication</i></p>"#));
}
