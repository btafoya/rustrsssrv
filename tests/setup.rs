mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn setup_page_renders_when_no_users() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/setup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(text.contains("Create the first admin"));
}

#[tokio::test]
async fn setup_creates_first_user_and_redirects() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let form = "email=admin%40example.com&password=Password123!&password_confirmation=Password123!";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/setup")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers()["location"], "/login");

    // Second request should redirect to /
    let res = app
        .oneshot(
            Request::builder()
                .uri("/setup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers()["location"], "/");
}

#[tokio::test]
async fn setup_rejects_mismatched_passwords() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let form = "email=admin%40example.com&password=Password123!&password_confirmation=Password456!";
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/setup")
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
