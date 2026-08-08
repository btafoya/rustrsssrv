mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

async fn create_user(app: &axum::Router, email: &str, password: &str) {
    let form = format!(
        "email={}&password={}&password_confirmation={}",
        urlencoding::encode(email),
        urlencoding::encode(password),
        urlencoding::encode(password)
    );
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
    assert!(res.status().is_success() || res.status() == StatusCode::SEE_OTHER);
}

async fn login(app: &axum::Router, email: &str, password: &str) -> String {
    let body = json!({"email": email, "password": password});
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["access_token"].as_str().unwrap().to_string()
}

fn auth_request(token: &str, uri: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn web_dashboard_redirects_to_setup_when_no_users() {
    let (app, _pool, _dir) = common::app_with_db().await;
    let res = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers().get("location").unwrap(), "/setup");
}

#[tokio::test]
async fn web_dashboard_redirects_to_login_after_setup_when_unauthenticated() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;

    let res = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers().get("location").unwrap(), "/login");
}

#[tokio::test]
async fn web_dashboard_renders() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Dashboard"));
}

#[tokio::test]
async fn web_dashboard_renders_with_login_cookie() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "cookie@example.com", "Password123!").await;

    let body = json!({"email": "cookie@example.com", "password": "Password123!"});
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let cookie = res
        .headers()
        .get("set-cookie")
        .expect("set-cookie header")
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("access_token="));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/")
                .header("Cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Dashboard"));
}

#[tokio::test]
async fn web_articles_renders() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/articles"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Articles"));
}

#[tokio::test]
async fn web_feeds_renders() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/feeds"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Feeds"));
}

#[tokio::test]
async fn web_search_renders() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/search?q=rss"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Search"));
}

#[tokio::test]
async fn web_settings_renders() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/settings"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Settings"));
}
