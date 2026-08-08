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

fn auth_request(token: &str, method: &str, uri: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap()
}

fn make_crawler(pool: &sqlx::SqlitePool) -> CrawlerService {
    let client = reqwest::Client::new();
    let cleaner = CleanerService::new(client.clone());
    let media = MediaService::new(pool.clone(), client.clone());
    CrawlerService::new(pool.clone(), client, cleaner, media)
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

async fn mock_origin_server(content: &'static str) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new().route(
        "/",
        axum::routing::get(move || async move {
            (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/html")],
                content,
            )
        }),
    );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
}

#[tokio::test]
async fn article_html_is_cleaned_to_markdown() {
    let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>https://example.com</link>
    <item>
      <title>Clean Post</title>
      <link>https://example.com/clean</link>
      <description><![CDATA[
        <h1>Title</h1>
        <p>Hello <strong>world</strong>.</p>
        <p>This paragraph adds enough words to the article so that the cleaner does not treat the feed body as truncated and fetch the origin page. The word count must exceed the truncation threshold by a comfortable margin. Here are more words. Here are even more words to be absolutely certain that the resulting markdown is long enough. The cleaner uses a word count threshold to decide whether the feed provided enough content or whether it needs to fetch the origin page. We want to test the html to markdown conversion directly without involving the origin fetch path.</p>
        <script>alert('xss');</script>
        <div class="ad">Buy now!</div>
      ]]></description>
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

    let row = sqlx::query!(
        "SELECT markdown_content, raw_html, summary FROM articles WHERE url = ?",
        "https://example.com/clean"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(row.markdown_content.contains("Title"));
    assert!(row.markdown_content.contains("**world**") || row.markdown_content.contains("world"));
    assert!(!row.markdown_content.contains("alert('xss')"));
    assert!(!row.markdown_content.contains("Buy now!"));
}

#[tokio::test]
async fn truncated_feed_falls_back_to_origin_page() {
    let origin = r#"<!DOCTYPE html>
<html>
<head><title>Full Article</title></head>
<body>
  <article>
    <h1>Full Article</h1>
    <p>This is the full article content that is much longer than the truncated feed summary. It has enough words to pass the truncation threshold easily and should be extracted by readability.</p>
    <p>Here is another paragraph with even more text so that the word count is definitely above fifty words when converted to markdown.</p>
  </article>
</body>
</html>
"#;

    let origin_addr = mock_origin_server(origin).await;
    let origin_url = format!("http://{}/", origin_addr);

    let rss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>{}</link>
    <item>
      <title>Short Post</title>
      <link>{}</link>
      <description><p>Short summary.</p></description>
    </item>
  </channel>
</rss>
"#,
        origin_url, origin_url
    );

    let rss: &'static str = Box::leak(rss.into_boxed_str());
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

    let row = sqlx::query!(
        "SELECT markdown_content, raw_html FROM articles WHERE url = ?",
        origin_url
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        row.markdown_content
            .contains("This is the full article content")
    );
    assert!(
        row.markdown_content
            .contains("much longer than the truncated feed summary")
    );
    let raw_html = row.raw_html.unwrap_or_default();
    assert!(raw_html.contains("article"));
}

#[tokio::test]
async fn web_article_page_renders_sanitized_html() {
    let rss = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>https://example.com</link>
    <item>
      <title>Web Render Post</title>
      <link>https://example.com/web-render</link>
      <description><![CDATA[
        <h1>Web Render Post</h1>
        <p>This is a paragraph with <strong>bold</strong> text. We need enough words here to exceed the truncation threshold so the origin page is not fetched. More words more words more words more words more words more words more words more words. Even more words are added here to guarantee the cleaner does not trigger origin fetching. The threshold is fifty words and we want to be well above it.</p>
        <script>alert('xss');</script>
      ]]></description>
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

    let article_id = sqlx::query!(
        r#"SELECT id as "id!" FROM articles WHERE url = ?"#,
        "https://example.com/web-render"
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .id;

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            &format!("/articles/{}", article_id),
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8_lossy(&bytes);

    assert!(html.contains("Web Render Post"));
    assert!(html.contains("<strong>bold</strong>") || html.contains("<strong>"));
    assert!(!html.contains("alert('xss')"));
}
