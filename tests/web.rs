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

async fn subscribe(app: &axum::Router, token: &str, url: &str) -> i64 {
    let body = json!({"url": url});
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/feeds")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["id"].as_i64().unwrap()
}

async fn insert_article(pool: &sqlx::SqlitePool, url: &str, title: &str, feed_id: i64) -> i64 {
    let now = chrono::Utc::now().timestamp_millis();
    let article_id = sqlx::query!(
        r#"
        INSERT INTO articles (url, title, markdown_content, fetched_at, updated_at)
        VALUES (?, ?, ?, ?, ?)
        RETURNING id
        "#,
        url,
        title,
        "",
        now,
        now
    )
    .fetch_one(pool)
    .await
    .unwrap()
    .id;

    sqlx::query!(
        "INSERT OR IGNORE INTO article_feeds (article_id, feed_id, first_seen_at) VALUES (?, ?, ?)",
        article_id,
        feed_id,
        now
    )
    .execute(pool)
    .await
    .unwrap();

    sqlx::query!(
        "INSERT OR IGNORE INTO read_states (user_id, article_id, created_at, updated_at) SELECT user_id, ?, ?, ? FROM subscriptions WHERE feed_id = ?",
        article_id,
        now,
        now,
        feed_id
    )
    .execute(pool)
    .await
    .unwrap();

    article_id
}

async fn star_article(app: &axum::Router, token: &str, article_id: i64) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/articles/{}/star", article_id))
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn static_css_serves_real_stylesheet() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/static/css/tailadmin.css")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "text/css"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!body.contains("<!DOCTYPE html>"));
    assert!(body.contains("tailwind") || body.contains("--tw") || body.contains(".bg-blue-600"));
}

#[tokio::test]
async fn static_js_serves_jquery_and_star_scripts() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/static/js/jquery.min.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/javascript"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("jQuery") || body.contains("jquery"));

    let res = app
        .oneshot(
            Request::builder()
                .uri("/static/js/star.js")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers().get("content-type").unwrap().to_str().unwrap(),
        "application/javascript"
    );
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(body.contains("star-toggle"));
    assert!(body.contains("$.ajax"));
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
async fn web_dashboard_unread_count_exceeds_preview_page_size() {
    // The dashboard's preview list is capped at 10 items; unread_count must
    // report the true total, not len(preview) + 1.
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    for i in 0..15 {
        insert_article(
            &pool,
            &format!("https://example.com/post-{}", i),
            &format!("Post {}", i),
            feed_id,
        )
        .await;
    }

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("15 unread articles"), "html: {html}");
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
async fn web_articles_filter_starred() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    let _plain_id = insert_article(&pool, "https://example.com/plain", "Plain Post", feed_id).await;
    let starred_id = insert_article(
        &pool,
        "https://example.com/starred",
        "Starred Post",
        feed_id,
    )
    .await;
    star_article(&app, &token, starred_id).await;

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/articles?filter=starred"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Starred Post"));
    assert!(!html.contains("Plain Post"));
    assert!(html.contains(r#"value="starred" selected"#));
}

#[tokio::test]
async fn web_articles_filter_by_feed_and_persists_default() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;
    let feed_a = subscribe(&app, &token, "https://example.com/a.xml").await;
    let feed_b = subscribe(&app, &token, "https://example.com/b.xml").await;

    insert_article(&pool, "https://example.com/a/1", "From Feed A", feed_a).await;
    insert_article(&pool, "https://example.com/b/1", "From Feed B", feed_b).await;

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            &format!("/articles?feed_id={}&filter=all", feed_a),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("From Feed A"));
    assert!(!html.contains("From Feed B"));
    assert!(html.contains(&format!(r#"value="{}" selected"#, feed_a)));

    // Explicit feed selection persists as the user's default.
    let res = app
        .clone()
        .oneshot(auth_request(&token, "/api/v1/users/me"))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let user: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(user["default_feed_id"], feed_a);

    // Revisiting without a feed_id param falls back to the saved default.
    let res = app
        .clone()
        .oneshot(auth_request(&token, "/articles?filter=all"))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("From Feed A"));
    assert!(!html.contains("From Feed B"));

    // Explicitly picking "All Feeds" clears the saved default.
    let res = app
        .clone()
        .oneshot(auth_request(&token, "/articles?feed_id=&filter=all"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("From Feed A"));
    assert!(html.contains("From Feed B"));

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/api/v1/users/me"))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let user: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(user["default_feed_id"].is_null());
}

#[tokio::test]
async fn web_articles_legacy_is_read_empty_shows_all() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    let article_id = insert_article(&pool, "https://example.com/post", "Any Post", feed_id).await;
    // Mark it read so it would be hidden under the default unread filter.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/articles/{}/read", article_id))
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    // Legacy URL with empty is_read should show all articles.
    let res = app
        .clone()
        .oneshot(auth_request(&token, "/articles?is_read=&sort=oldest_first"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Any Post"));
}

#[tokio::test]
async fn web_articles_legacy_is_read_bool() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    let read_id = insert_article(&pool, "https://example.com/read", "Read Post", feed_id).await;
    let _unread_id =
        insert_article(&pool, "https://example.com/unread", "Unread Post", feed_id).await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/articles/{}/read", read_id))
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/articles?is_read=true"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Read Post"));
    assert!(!html.contains("Unread Post"));

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/articles?is_read=false"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Unread Post"));
    assert!(!html.contains("Read Post"));
}

#[tokio::test]
async fn web_articles_default_filter_starred() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/users/me")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"default_filter": "starred"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;
    let _plain_id = insert_article(&pool, "https://example.com/plain", "Plain Post", feed_id).await;
    let starred_id = insert_article(
        &pool,
        "https://example.com/starred",
        "Starred Post",
        feed_id,
    )
    .await;
    star_article(&app, &token, starred_id).await;

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/articles"))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Starred Post"));
    assert!(!html.contains("Plain Post"));
    assert!(html.contains(r#"value="starred" selected"#));
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
    assert!(html.contains("Add feed"));
    assert!(html.contains("Discover feeds"));
    assert!(html.contains("Import OPML"));
}

#[tokio::test]
async fn web_add_feed_creates_subscription() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let form = "url=https%3A%2F%2Fexample.com%2Ffeed.xml";
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/feeds/add")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers().get("location").unwrap(), "/feeds");

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/feeds"))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("https://example.com/feed.xml"));
}

#[tokio::test]
async fn web_delete_feed_removes_subscription() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let form = "url=https%3A%2F%2Fexample.com%2Ffeed.xml";
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/feeds/add")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/feeds"))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("https://example.com/feed.xml"));

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/feeds/1/delete")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);

    let res = app
        .clone()
        .oneshot(auth_request(&token, "/feeds"))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("No feeds yet."));
}

#[tokio::test]
async fn web_refresh_feed_accepts() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let form = "url=https%3A%2F%2Fexample.com%2Ffeed.xml";
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/feeds/add")
                .header("Authorization", format!("Bearer {}", token))
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap(),
        )
        .await
        .unwrap();

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/feeds/1/refresh")
                .header("Authorization", format!("Bearer {}", token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn web_import_opml_adds_feeds() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "web@example.com", "Password123!").await;
    let token = login(&app, "web@example.com", "Password123!").await;

    let boundary = "----WebKitFormBoundary7MA4YWxkTrZu0gW";
    let body = format!(
        "--{}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"subs.opml\"\r\nContent-Type: application/xml\r\n\r\n{}\r\n--{}--\r\n",
        boundary,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<opml version="2.0">
  <head><title>Subs</title></head>
  <body>
    <outline text="Example" title="Example" type="rss" xmlUrl="https://example.com/feed.xml" />
  </body>
</opml>"#,
        boundary
    );

    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/feeds/import")
                .header("Authorization", format!("Bearer {}", token))
                .header(
                    "Content-Type",
                    format!("multipart/form-data; boundary={}", boundary),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(html.contains("Imported 1 of 1 feeds"));
    assert!(html.contains("https://example.com/feed.xml"));
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
