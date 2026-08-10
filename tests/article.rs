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

fn auth_request(token: &str, method: &str, uri: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("Authorization", format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .body(body)
        .unwrap()
}

async fn subscribe(app: &axum::Router, token: &str, url: &str) -> i64 {
    let body = json!({"url": url});
    let res = app
        .clone()
        .oneshot(auth_request(
            token,
            "POST",
            "/api/v1/feeds",
            Body::from(body.to_string()),
        ))
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

async fn insert_article_with_published_at(
    pool: &sqlx::SqlitePool,
    url: &str,
    title: &str,
    feed_id: i64,
    published_at: i64,
) -> i64 {
    let now = chrono::Utc::now().timestamp_millis();
    let article_id = sqlx::query!(
        r#"
        INSERT INTO articles (url, title, markdown_content, published_at, fetched_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        RETURNING id
        "#,
        url,
        title,
        "",
        published_at,
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

#[tokio::test]
async fn articles_require_auth() {
    let (app, _pool, _dir) = common::app_with_db().await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/articles")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_articles_scoped_to_subscriptions() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    let article_id = insert_article(&pool, "https://example.com/post", "Post", feed_id).await;

    // Another feed and article that the user is not subscribed to.
    let other_feed_id = sqlx::query!(
        "INSERT INTO feeds (url, fetch_interval_minutes, next_fetch_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
        "https://other.example.com/feed.xml",
        15,
        0,
        0,
        0
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .id;
    let _other_article_id = insert_article(
        &pool,
        "https://other.example.com/post",
        "Other Post",
        other_feed_id,
    )
    .await;

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/articles",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], article_id);
    assert_eq!(items[0]["feed_id"], feed_id);
}

#[tokio::test]
async fn get_article_requires_subscription() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let other_feed_id = sqlx::query!(
        "INSERT INTO feeds (url, fetch_interval_minutes, next_fetch_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
        "https://other.example.com/feed.xml",
        15,
        0,
        0,
        0
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .id;
    let article_id = insert_article(
        &pool,
        "https://other.example.com/post",
        "Other Post",
        other_feed_id,
    )
    .await;

    let uri = format!("/api/v1/articles/{}", article_id);
    let res = app
        .clone()
        .oneshot(auth_request(&token, "GET", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_articles_sorts_by_published_at_not_insertion_order() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    // RSS feeds conventionally list items newest-first, and the crawler inserts
    // them in that order — so insertion order is the reverse of publish order.
    // A real fix must sort by published_at, not by id/insertion order.
    let base = 1_700_000_000_000_i64;
    insert_article_with_published_at(&pool, "https://example.com/c", "Newest", feed_id, base + 2)
        .await;
    insert_article_with_published_at(&pool, "https://example.com/b", "Middle", feed_id, base + 1)
        .await;
    insert_article_with_published_at(&pool, "https://example.com/a", "Oldest", feed_id, base).await;

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/articles?sort=newest_first",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let titles: Vec<&str> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["Newest", "Middle", "Oldest"]);

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/articles?sort=oldest_first",
            Body::empty(),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let titles: Vec<&str> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["Oldest", "Middle", "Newest"]);
}

#[tokio::test]
async fn list_articles_paginates() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    for i in 0..3 {
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
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/articles?limit=2",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert!(page["has_more"].as_bool().unwrap());
    let cursor = page["next_cursor"].as_i64().unwrap();

    let uri = format!("/api/v1/articles?limit=2&cursor={}", cursor);
    let res = app
        .clone()
        .oneshot(auth_request(&token, "GET", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert!(!page["has_more"].as_bool().unwrap());
}

async fn insert_article_with_content(
    pool: &sqlx::SqlitePool,
    url: &str,
    title: &str,
    content: &str,
    feed_id: i64,
) -> i64 {
    let now = chrono::Utc::now().timestamp_millis();
    let article_id = sqlx::query!(
        r#"
        INSERT INTO articles (url, title, markdown_content, fetched_at, updated_at)
        VALUES (?, ?, ?, ?, ?)
        RETURNING id
        "#,
        url,
        title,
        content,
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

#[tokio::test]
async fn mark_read_and_unread() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "reader@example.com", "Password123!").await;
    let token = login(&app, "reader@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;
    let article_id =
        insert_article(&pool, "https://example.com/read-post", "Read Post", feed_id).await;

    let uri = format!("/api/v1/articles/{}/read", article_id);
    let res = app
        .clone()
        .oneshot(auth_request(&token, "POST", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            &format!("/api/v1/articles/{}", article_id),
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let article: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(article["is_read"].as_bool().unwrap());

    let uri = format!("/api/v1/articles/{}/unread", article_id);
    let res = app
        .clone()
        .oneshot(auth_request(&token, "POST", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            &format!("/api/v1/articles/{}", article_id),
            Body::empty(),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let article: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(!article["is_read"].as_bool().unwrap());
}

#[tokio::test]
async fn mark_starred_and_unstarred() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "starrer@example.com", "Password123!").await;
    let token = login(&app, "starrer@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;
    let article_id =
        insert_article(&pool, "https://example.com/star-post", "Star Post", feed_id).await;

    let uri = format!("/api/v1/articles/{}/star", article_id);
    let res = app
        .clone()
        .oneshot(auth_request(&token, "POST", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            &format!("/api/v1/articles/{}", article_id),
            Body::empty(),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let article: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(article["is_starred"].as_bool().unwrap());

    let uri = format!("/api/v1/articles/{}/unstar", article_id);
    let res = app
        .clone()
        .oneshot(auth_request(&token, "POST", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            &format!("/api/v1/articles/{}", article_id),
            Body::empty(),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let article: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(!article["is_starred"].as_bool().unwrap());
}

#[tokio::test]
async fn mark_read_rejects_unsubscribed_article() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    let other_feed_id = sqlx::query!(
        "INSERT INTO feeds (url, fetch_interval_minutes, next_fetch_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
        "https://other.example.com/feed.xml",
        15,
        0,
        0,
        0
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .id;
    let article_id = insert_article(
        &pool,
        "https://other.example.com/post",
        "Other Post",
        other_feed_id,
    )
    .await;

    let uri = format!("/api/v1/articles/{}/read", article_id);
    let res = app
        .clone()
        .oneshot(auth_request(&token, "POST", &uri, Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn search_requires_auth() {
    let (app, _pool, _dir) = common::app_with_db().await;
    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_articles_pages_backward_with_prev_cursor() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    for i in 0..3 {
        insert_article(
            &pool,
            &format!("https://example.com/post-{}", i),
            &format!("Post {}", i),
            feed_id,
        )
        .await;
    }

    // Page 1 (oldest_first default): 2 items, a next cursor, no prev cursor.
    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/articles?limit=2",
            Body::empty(),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page1: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(page1["prev_cursor"].is_null());
    let next_cursor = page1["next_cursor"].as_i64().unwrap();

    // Page 2: 1 remaining item, no next cursor, a prev cursor pointing back.
    let uri = format!("/api/v1/articles?limit=2&cursor={}", next_cursor);
    let res = app
        .clone()
        .oneshot(auth_request(&token, "GET", &uri, Body::empty()))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page2: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(page2["next_cursor"].is_null());
    let prev_cursor = page2["prev_cursor"].as_i64().unwrap();

    // Following prev_cursor with direction=prev reconstructs page 1 exactly.
    let uri = format!(
        "/api/v1/articles?limit=2&cursor={}&direction=prev",
        prev_cursor
    );
    let res = app
        .clone()
        .oneshot(auth_request(&token, "GET", &uri, Body::empty()))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let back: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

    let back_ids: Vec<i64> = back["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_i64().unwrap())
        .collect();
    let page1_ids: Vec<i64> = page1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_i64().unwrap())
        .collect();
    assert_eq!(back_ids, page1_ids);
    assert!(back["prev_cursor"].is_null());
    assert_eq!(back["next_cursor"].as_i64().unwrap(), next_cursor);
}

#[tokio::test]
async fn article_list_page_saves_explicit_filter_but_not_legacy_params() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "user@example.com", "Password123!").await;
    let token = login(&app, "user@example.com", "Password123!").await;

    // Legacy bookmark-style param must not overwrite the saved default.
    let res = app
        .clone()
        .oneshot(auth_request(&token, "GET", "/?is_read=true", Body::empty()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/users/me",
            Body::empty(),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let user: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(user["default_filter"], "unread");

    // Explicit filter/sort submitted via the page's own form is saved.
    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/?filter=starred&sort=newest_first",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/users/me",
            Body::empty(),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let user: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(user["default_filter"], "starred");
    assert_eq!(user["default_sort_order"], "newest_first");
}

#[tokio::test]
async fn search_matches_subscribed_articles() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "searcher@example.com", "Password123!").await;
    let token = login(&app, "searcher@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;

    let matched_id = insert_article_with_content(
        &pool,
        "https://example.com/alpha",
        "Alpha Post",
        "unique search term alpha",
        feed_id,
    )
    .await;
    let _other_id = insert_article_with_content(
        &pool,
        "https://example.com/beta",
        "Beta Post",
        "something unrelated",
        feed_id,
    )
    .await;

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/search?q=alpha&limit=10",
            Body::empty(),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], matched_id);
}

#[tokio::test]
async fn bulk_mark_read_by_ids() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "bulk-reader@example.com", "Password123!").await;
    let token = login(&app, "bulk-reader@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;
    let id1 = insert_article(&pool, "https://example.com/bulk-1", "Bulk One", feed_id).await;
    let id2 = insert_article(&pool, "https://example.com/bulk-2", "Bulk Two", feed_id).await;
    let id3 = insert_article(&pool, "https://example.com/bulk-3", "Bulk Three", feed_id).await;

    let body = json!({"action": "read", "article_ids": [id1, id2]});
    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "POST",
            "/api/v1/articles/bulk",
            Body::from(body.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["affected"], 2);

    for (id, expect_read) in [(id1, true), (id2, true), (id3, false)] {
        let res = app
            .clone()
            .oneshot(auth_request(
                &token,
                "GET",
                &format!("/api/v1/articles/{}", id),
                Body::empty(),
            ))
            .await
            .unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let article: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(article["is_read"].as_bool().unwrap(), expect_read);
    }
}

#[tokio::test]
async fn bulk_hide_excludes_article_from_list() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "bulk-hider@example.com", "Password123!").await;
    let token = login(&app, "bulk-hider@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;
    let hidden_id = insert_article(&pool, "https://example.com/hide-me", "Hide Me", feed_id).await;
    let visible_id = insert_article(&pool, "https://example.com/keep-me", "Keep Me", feed_id).await;

    let body = json!({"action": "hide", "article_ids": [hidden_id]});
    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "POST",
            "/api/v1/articles/bulk",
            Body::from(body.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "GET",
            "/api/v1/articles",
            Body::empty(),
        ))
        .await
        .unwrap();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let ids: Vec<i64> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_i64().unwrap())
        .collect();
    assert!(!ids.contains(&hidden_id));
    assert!(ids.contains(&visible_id));
}

#[tokio::test]
async fn bulk_by_filter_applies_only_to_matching_articles() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "bulk-filter@example.com", "Password123!").await;
    let token = login(&app, "bulk-filter@example.com", "Password123!").await;
    let feed_id = subscribe(&app, &token, "https://example.com/feed.xml").await;
    let unread_id =
        insert_article(&pool, "https://example.com/unread-1", "Unread One", feed_id).await;
    let read_id = insert_article(&pool, "https://example.com/read-1", "Read One", feed_id).await;

    let mark_read_uri = format!("/api/v1/articles/{}/read", read_id);
    app.clone()
        .oneshot(auth_request(&token, "POST", &mark_read_uri, Body::empty()))
        .await
        .unwrap();

    let body = json!({"action": "star", "filter": {"is_read": false}});
    let res = app
        .clone()
        .oneshot(auth_request(
            &token,
            "POST",
            "/api/v1/articles/bulk",
            Body::from(body.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(result["affected"], 1);

    for (id, expect_starred) in [(unread_id, true), (read_id, false)] {
        let res = app
            .clone()
            .oneshot(auth_request(
                &token,
                "GET",
                &format!("/api/v1/articles/{}", id),
                Body::empty(),
            ))
            .await
            .unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let article: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(article["is_starred"].as_bool().unwrap(), expect_starred);
    }
}

#[tokio::test]
async fn bulk_requires_article_ids_or_filter() {
    let (app, _pool, _dir) = common::app_with_db().await;
    create_user(&app, "bulk-empty@example.com", "Password123!").await;
    let token = login(&app, "bulk-empty@example.com", "Password123!").await;

    let body = json!({"action": "read"});
    let res = app
        .oneshot(auth_request(
            &token,
            "POST",
            "/api/v1/articles/bulk",
            Body::from(body.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn bulk_rejects_unsubscribed_article() {
    let (app, pool, _dir) = common::app_with_db().await;
    create_user(&app, "bulk-unsub@example.com", "Password123!").await;
    let token = login(&app, "bulk-unsub@example.com", "Password123!").await;

    let other_feed_id = sqlx::query!(
        "INSERT INTO feeds (url, fetch_interval_minutes, next_fetch_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
        "https://other.example.com/feed.xml",
        15,
        0,
        0,
        0
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .id;
    let article_id = insert_article(
        &pool,
        "https://other.example.com/bulk-post",
        "Other Bulk Post",
        other_feed_id,
    )
    .await;

    let body = json!({"action": "read", "article_ids": [article_id]});
    let res = app
        .oneshot(auth_request(
            &token,
            "POST",
            "/api/v1/articles/bulk",
            Body::from(body.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
