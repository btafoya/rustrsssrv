mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

macro_rules! auth_request {
    ($token:expr, $method:expr, $uri:expr, $body:expr) => {
        Request::builder()
            .method($method)
            .uri($uri)
            .header("Authorization", format!("Bearer {}", $token))
            .header("Content-Type", "application/json")
            .body($body)
            .unwrap()
    };
    ($token:expr, $method:expr, $uri:expr, $body:expr, $ct:expr) => {
        Request::builder()
            .method($method)
            .uri($uri)
            .header("Authorization", format!("Bearer {}", $token))
            .header("Content-Type", $ct)
            .body($body)
            .unwrap()
    };
}

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

#[tokio::test]
async fn feeds_require_auth() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/feeds")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn create_feed() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let body = json!({"url": "https://example.com/feed.xml"});
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["url"], "https://example.com/feed.xml");
    assert!(json["id"].as_i64().is_some());
}

#[tokio::test]
async fn create_feed_rejects_invalid_url() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let body = json!({"url": "not-a-url"});
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_and_get_feed() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let body = json!({"url": "https://example.com/feed.xml"});
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id = created["id"].as_i64().unwrap();

    let res = app
        .clone()
        .oneshot(auth_request!(&token, "GET", "/api/v1/feeds", Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["items"][0]["id"], feed_id);

    let uri = format!("/api/v1/feeds/{}", feed_id);
    let res = app
        .clone()
        .oneshot(auth_request!(&token, "GET", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let feed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(feed["url"], "https://example.com/feed.xml");
}

#[tokio::test]
async fn patch_feed() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let body = json!({"url": "https://example.com/feed.xml"});
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id = created["id"].as_i64().unwrap();

    let body = json!({"title": "My Feed", "fetch_interval_minutes": 60});
    let uri = format!("/api/v1/feeds/{}", feed_id);
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "PATCH",
            &uri,
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let feed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(feed["title"], "My Feed");
    assert_eq!(feed["fetch_interval_minutes"], 60);
}

#[tokio::test]
async fn patch_feed_rejects_invalid_interval() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let body = json!({"url": "https://example.com/feed.xml"});
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id = created["id"].as_i64().unwrap();

    let body = json!({"fetch_interval_minutes": 123});
    let uri = format!("/api/v1/feeds/{}", feed_id);
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "PATCH",
            &uri,
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn delete_feed_unsubscribes() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let body = json!({"url": "https://example.com/feed.xml"});
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id = created["id"].as_i64().unwrap();

    let uri = format!("/api/v1/feeds/{}", feed_id);
    let res = app
        .clone()
        .oneshot(auth_request!(&token, "DELETE", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(auth_request!(&token, "GET", "/api/v1/feeds", Body::empty()))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(page["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn import_and_export_opml() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let opml = r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head><title>Subs</title></head>
  <body>
    <outline text="Example" title="Example" type="rss" xmlUrl="https://example.com/feed.xml" />
  </body>
</opml>
"#;
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "POST",
            "/api/v1/feeds/import/opml",
            Body::from(opml),
            "application/xml"
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["imported"], 1);
    assert_eq!(result["failed"], 0);

    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "GET",
            "/api/v1/feeds/export/opml",
            Body::empty()
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "application/xml");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let xml = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(xml.contains("https://example.com/feed.xml"));
}

#[tokio::test]
async fn discover_requires_auth() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let body = json!({"url": "https://example.com/"});
    let res = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/feeds/discover")
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn discover_rejects_invalid_url() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let body = json!({"url": "not-a-url"});
    let res = app
        .clone()
        .oneshot(auth_request!(
            &token,
            "POST",
            "/api/v1/feeds/discover",
            Body::from(body.to_string())
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
