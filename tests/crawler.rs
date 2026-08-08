mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use rustrsssrv::services::CleanerService;
use rustrsssrv::services::MediaService;
use rustrsssrv::services::crawler::{CrawlerService, FeedRow};
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

fn make_crawler(pool: &sqlx::SqlitePool) -> CrawlerService {
    let client = reqwest::Client::new();
    let cleaner = CleanerService::new(client.clone());
    let media = MediaService::new(pool.clone(), client.clone());
    CrawlerService::new(pool.clone(), client, cleaner, media)
}

fn auth_request(token: &str, method: &str, uri: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap()
}

async fn mock_feed_server(body: &'static str) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/feed.xml",
        axum::routing::get(move || async move {
            (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "application/rss+xml")],
                body,
            )
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn fetch_rss_feed_stores_articles_and_read_states() {
    let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>https://example.com</link>
    <description>An example feed</description>
    <item>
      <title>First Post</title>
      <link>https://example.com/first</link>
      <description>The first post.</description>
    </item>
  </channel>
</rss>
"#;
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let addr = mock_feed_server(rss).await;
    let feed_url = format!("http://{}/feed.xml", addr);

    let body = json!({"url": feed_url});
    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id = created["id"].as_i64().unwrap();

    // Trigger the fetch synchronously for the test.
    let crawler = make_crawler(&pool);
    crawler
        .fetch_feed(FeedRow {
            id: feed_id,
            url: feed_url,
            last_etag: None,
            last_modified: None,
        })
        .await
        .unwrap();

    let article = sqlx::query!(
        "SELECT id, url, title FROM articles WHERE url = ?",
        "https://example.com/first"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(article.title, "First Post");

    let linked = sqlx::query!(
        r#"SELECT 1 as "found!: i64" FROM article_feeds WHERE article_id = ? AND feed_id = ?"#,
        article.id,
        feed_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(linked.found, 1);

    let read = sqlx::query!(
        r#"SELECT 1 as "found!: i64" FROM read_states WHERE user_id = (SELECT id FROM users WHERE email = ?) AND article_id = ?"#,
        "user@example.com",
        article.id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(read.found, 1);
}

#[tokio::test]
async fn duplicate_article_url_upserts_existing_row() {
    let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>https://example.com</link>
    <item>
      <title>Original Title</title>
      <link>https://example.com/post</link>
    </item>
  </channel>
</rss>
"#;
    let rss2 = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>https://example.com</link>
    <item>
      <title>Updated Title</title>
      <link>https://example.com/post</link>
    </item>
  </channel>
</rss>
"#;
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let addr = mock_feed_server(rss).await;
    let feed_url = format!("http://{}/feed.xml", addr);

    let body = json!({"url": feed_url});
    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string()),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id = created["id"].as_i64().unwrap();

    let crawler = make_crawler(&pool);
    crawler
        .fetch_feed(FeedRow {
            id: feed_id,
            url: feed_url.clone(),
            last_etag: None,
            last_modified: None,
        })
        .await
        .unwrap();

    let first = sqlx::query!(
        "SELECT title FROM articles WHERE url = ?",
        "https://example.com/post"
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(first.title, "Original Title");

    // Fetch a different feed with the same article URL.
    let addr2 = mock_feed_server(rss2).await;
    let feed_url2 = format!("http://{}/feed.xml", addr2);
    let body2 = json!({"url": feed_url2});
    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "POST",
            "/api/v1/feeds",
            Body::from(body2.to_string()),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let created2: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let feed_id2 = created2["id"].as_i64().unwrap();

    crawler
        .fetch_feed(FeedRow {
            id: feed_id2,
            url: feed_url2,
            last_etag: None,
            last_modified: None,
        })
        .await
        .unwrap();

    let rows = sqlx::query!(
        "SELECT id, title FROM articles WHERE url = ?",
        "https://example.com/post"
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "Updated Title");
}
