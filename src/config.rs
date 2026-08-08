use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub port: u16,
    pub enable_crawler: bool,
    pub log_dir: String,
    pub rust_log: String,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./data/rustrsssrv.db".into()),
            jwt_secret: env::var("JWT_SECRET").unwrap_or_else(|_| {
                eprintln!("JWT_SECRET not set; using insecure development secret");
                "dev-secret-do-not-use-in-production".into()
            }),
            port: env::var("PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(9119),
            enable_crawler: env::var("ENABLE_CRAWLER")
                .map(|s| s.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            log_dir: env::var("LOG_DIR").unwrap_or_else(|_| "./logs".into()),
            rust_log: env::var("RUST_LOG").unwrap_or_else(|_| "warn".into()),
        }
    }

    pub fn for_test(db_url: &str) -> Self {
        Self {
            database_url: db_url.into(),
            jwt_secret: "test-secret".into(),
            port: 0,
            enable_crawler: false,
            log_dir: "./logs".into(),
            rust_log: "error".into(),
        }
    }
}
