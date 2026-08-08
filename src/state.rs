use std::sync::Arc;

use sqlx::SqlitePool;

use crate::config::Config;
use crate::services::{ArticleService, AuthService, CrawlerService, FeedService, UserService};

pub struct AppStateInner {
    pub config: Config,
    pub pool: SqlitePool,
    pub auth: AuthService,
    pub users: UserService,
    pub feeds: FeedService,
    pub articles: ArticleService,
    pub crawler: CrawlerService,
}

pub type AppState = Arc<AppStateInner>;

impl AppStateInner {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        let crawler = CrawlerService::new(pool.clone());
        Self {
            auth: AuthService::new(pool.clone(), config.jwt_secret.clone()),
            users: UserService::new(pool.clone()),
            feeds: FeedService::new(pool.clone(), crawler.clone()),
            articles: ArticleService::new(pool.clone()),
            crawler,
            pool,
            config,
        }
    }
}
