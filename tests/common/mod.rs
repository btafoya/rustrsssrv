use std::sync::Arc;

use rustrsssrv::config::Config;
use rustrsssrv::handlers::build_app;
use rustrsssrv::state::AppStateInner;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

pub async fn app_with_db() -> (axum::Router, sqlx::SqlitePool, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    std::fs::create_dir_all(temp_dir.path()).unwrap();

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let db_url = format!("sqlite:{}", db_path.display());
    let config = Config::for_test(&db_url);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap_or_else(|e| panic!("failed to connect to {}: {}", db_url, e));

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let state = Arc::new(AppStateInner::new(config, pool.clone()));
    (build_app(state), pool, temp_dir)
}
