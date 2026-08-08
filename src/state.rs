use std::sync::Arc;

use sqlx::SqlitePool;

use crate::config::Config;
use crate::services::{
    ArticleService, AuthService, CleanerService, CrawlerService, FeedService, MediaService,
    UserService,
};

pub struct AppStateInner {
    pub config: Config,
    pub pool: SqlitePool,
    pub auth: AuthService,
    pub users: UserService,
    pub feeds: FeedService,
    pub articles: ArticleService,
    pub crawler: CrawlerService,
    pub cleaner: CleanerService,
    pub media: MediaService,
}

pub type AppState = Arc<AppStateInner>;

impl AppStateInner {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!(
                env!("CARGO_PKG_NAME"),
                "/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let cleaner = CleanerService::new(client.clone());
        let media = MediaService::new(pool.clone(), client.clone());
        let crawler = CrawlerService::new(pool.clone(), client, cleaner.clone(), media.clone());
        Self {
            auth: AuthService::new(pool.clone(), config.jwt_secret.clone()),
            users: UserService::new(pool.clone()),
            feeds: FeedService::new(pool.clone(), crawler.clone()),
            articles: ArticleService::new(pool.clone()),
            crawler,
            cleaner,
            media,
            pool,
            config,
        }
    }
}
