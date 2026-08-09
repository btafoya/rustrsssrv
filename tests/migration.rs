use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

#[tokio::test]
async fn migration_002_preserves_subscriptions_and_feeds() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();

    // Apply only the initial migration.
    let migrator = sqlx::migrate!("./migrations");
    migrator.run(&pool).await.unwrap();

    let now = chrono::Utc::now().timestamp_millis();

    // Seed a user, feed, subscription, and article.
    let user_id = sqlx::query!(
        "INSERT INTO users (email, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?) RETURNING id",
        "user@example.com",
        "hash",
        now,
        now
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .id;

    let feed_id = sqlx::query!(
        "INSERT INTO feeds (url, fetch_interval_minutes, next_fetch_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
        "https://example.com/feed.xml",
        15,
        now,
        now,
        now
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .id;

    sqlx::query!(
        "INSERT INTO subscriptions (user_id, feed_id, created_at) VALUES (?, ?, ?)",
        user_id,
        feed_id,
        now
    )
    .execute(&pool)
    .await
    .unwrap();

    let article_id = sqlx::query!(
        "INSERT INTO articles (url, title, markdown_content, fetched_at, updated_at) VALUES (?, ?, ?, ?, ?) RETURNING id",
        "https://example.com/post",
        "Post",
        "",
        now,
        now
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .id;

    sqlx::query!(
        "INSERT INTO article_feeds (article_id, feed_id, first_seen_at) VALUES (?, ?, ?)",
        article_id,
        feed_id,
        now
    )
    .execute(&pool)
    .await
    .unwrap();

    // Re-run migrations (idempotent) to ensure 002 runs on top of existing data.
    let migrator = sqlx::migrate!("./migrations");
    migrator.run(&pool).await.unwrap();

    let feed_count = sqlx::query!("SELECT COUNT(*) as cnt FROM feeds")
        .fetch_one(&pool)
        .await
        .unwrap()
        .cnt;
    let sub_count = sqlx::query!("SELECT COUNT(*) as cnt FROM subscriptions")
        .fetch_one(&pool)
        .await
        .unwrap()
        .cnt;
    let article_count = sqlx::query!("SELECT COUNT(*) as cnt FROM articles")
        .fetch_one(&pool)
        .await
        .unwrap()
        .cnt;

    assert_eq!(feed_count, 1);
    assert_eq!(sub_count, 1);
    assert_eq!(article_count, 1);
}
