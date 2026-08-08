use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rustrsssrv::config::Config;
use rustrsssrv::handlers::build_app;
use rustrsssrv::state::AppStateInner;

#[tokio::main]
async fn main() {
    let config = Config::from_env();

    let log_file = tracing_appender::rolling::daily(&config.log_dir, "rustrsssrv.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(log_file);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.rust_log.clone().into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stdout))
        .init();

    let db_path = config
        .database_url
        .trim_start_matches("sqlite:")
        .trim_start_matches("//");
    if let Some(parent) = Path::new(db_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).expect("create database directory");
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(10)
        .connect_with(options)
        .await
        .expect("connect to database");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let state = Arc::new(AppStateInner::new(config.clone(), pool));

    let app = build_app(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind address");

    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.expect("serve");
}
