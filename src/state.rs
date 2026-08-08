use std::sync::Arc;

use sqlx::SqlitePool;

use crate::config::Config;
use crate::services::{AuthService, FeedService, UserService};

pub struct AppStateInner {
    pub config: Config,
    pub pool: SqlitePool,
    pub auth: AuthService,
    pub users: UserService,
    pub feeds: FeedService,
}

pub type AppState = Arc<AppStateInner>;

impl AppStateInner {
    pub fn new(config: Config, pool: SqlitePool) -> Self {
        Self {
            auth: AuthService::new(pool.clone(), config.jwt_secret.clone()),
            users: UserService::new(pool.clone()),
            feeds: FeedService::new(pool.clone()),
            pool,
            config,
        }
    }
}
