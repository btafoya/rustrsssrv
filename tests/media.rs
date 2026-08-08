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

async fn mock_asset_server(small: &'static [u8], large: &'static [u8]) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = axum::Router::new()
        .route(
            "/small.png",
            axum::routing::get(move || async move {
                (
                    axum::http::StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "image/png")],
                    small,
                )
            }),
        )
        .route(
            "/large.png",
            axum::routing::get(move || async move {
                (
                    axum::http::StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "image/png")],
                    large,
                )
            }),
        );
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    addr
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
async fn small_image_is_inlined_as_base64() {
    let small = vec![0x89u8, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    let large = vec![0u8; 129 * 1024];
    let addr = mock_asset_server(
        Box::leak(small.into_boxed_slice()),
        Box::leak(large.into_boxed_slice()),
    )
    .await;

    let rss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>https://example.com</link>
    <item>
      <title>Image Post</title>
      <link>https://example.com/image-post</link>
      <description><![CDATA[
        <p>Here is a small image. This text is here to make the article long enough that the cleaner does not treat the feed as truncated and fetch the origin page. We need enough words to exceed the truncation threshold so the media rewrite logic runs against the feed body directly.</p>
        <p>More words. More words. The threshold is fifty words. We must add enough filler text to be safe. Here is another sentence. And one more for good measure.</p>
        <img src="http://{}/small.png" alt="small" />
      ]]></description>
    </item>
  </channel>
</rss>
"#,
        addr
    );
    let rss: &'static str = Box::leak(rss.into_boxed_str());

    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let feed_addr = mock_feed_server(rss).await;
    let feed_url = format!("http://{}/feed.xml", feed_addr);

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
        "SELECT markdown_content FROM articles WHERE url = ?",
        "https://example.com/image-post"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(row.markdown_content.contains("data:image/png;base64,"));
    assert!(!row.markdown_content.contains("/api/v1/media/"));
}

#[tokio::test]
async fn large_image_is_proxied_by_hash() {
    let small = vec![0x89u8, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
    let large = vec![0u8; 129 * 1024];
    let addr = mock_asset_server(
        Box::leak(small.into_boxed_slice()),
        Box::leak(large.into_boxed_slice()),
    )
    .await;

    let rss = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example</title>
    <link>https://example.com</link>
    <item>
      <title>Image Post</title>
      <link>https://example.com/image-post</link>
      <description><![CDATA[
        <p>Here is a large image. This text is here to make the article long enough that the cleaner does not treat the feed as truncated and fetch the origin page. We need enough words to exceed the truncation threshold so the media rewrite logic runs against the feed body directly.</p>
        <p>More words. More words. The threshold is fifty words. We must add enough filler text to be safe. Here is another sentence. And one more for good measure.</p>
        <img src="http://{}/large.png" alt="large" />
      ]]></description>
    </item>
  </channel>
</rss>
"#,
        addr
    );
    let rss: &'static str = Box::leak(rss.into_boxed_str());

    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let feed_addr = mock_feed_server(rss).await;
    let feed_url = format!("http://{}/feed.xml", feed_addr);

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
        "SELECT markdown_content FROM articles WHERE url = ?",
        "https://example.com/image-post"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(!row.markdown_content.contains("data:image/png;base64,"));
    let media_url = row
        .markdown_content
        .split("/api/v1/media/")
        .nth(1)
        .expect("proxy url not found");
    let hash = media_url.split(')').next().unwrap().trim();

    let media_row = sqlx::query!(
        "SELECT mime_type, data FROM media WHERE content_hash = ?",
        hash
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(media_row.mime_type, "image/png");
    assert_eq!(media_row.data.len(), 129 * 1024);

    // Fetch via the API.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/media/{}", hash))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("image/png")
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.len(), 129 * 1024);
}
