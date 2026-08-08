CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    default_filter TEXT NOT NULL DEFAULT 'unread'
        CHECK (default_filter IN ('all', 'unread')),
    default_sort_order TEXT NOT NULL DEFAULT 'oldest_first'
        CHECK (default_sort_order IN ('oldest_first', 'newest_first')),
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE refresh_tokens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE feeds (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    title TEXT,
    description TEXT,
    site_url TEXT,
    fetch_interval_minutes INTEGER NOT NULL DEFAULT 15
        CHECK (fetch_interval_minutes IN (5, 15, 30, 60, 120, 240, 720, 1440)),
    last_fetched_at INTEGER,
    next_fetch_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    cache_until INTEGER,
    last_etag TEXT,
    last_modified TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    backoff_until INTEGER,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE subscriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    UNIQUE (user_id, feed_id)
);

CREATE TABLE articles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    summary TEXT,
    raw_html TEXT,
    markdown_content TEXT NOT NULL,
    published_at INTEGER,
    fetched_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE article_feeds (
    article_id INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    first_seen_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    PRIMARY KEY (article_id, feed_id)
);

CREATE TABLE read_states (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    article_id INTEGER NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    is_read INTEGER NOT NULL DEFAULT 0,
    read_at INTEGER,
    is_starred INTEGER NOT NULL DEFAULT 0,
    starred_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    UNIQUE (user_id, article_id)
);

CREATE TABLE media (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    content_hash TEXT NOT NULL UNIQUE,
    origin_url TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    data BLOB NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

CREATE TABLE feed_fetch_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    feed_id INTEGER NOT NULL REFERENCES feeds(id) ON DELETE CASCADE,
    fetched_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    status TEXT NOT NULL CHECK (status IN ('ok', 'error')),
    error_message TEXT,
    articles_found INTEGER
);

CREATE INDEX idx_feeds_next_fetch ON feeds(next_fetch_at);
CREATE INDEX idx_subscriptions_user ON subscriptions(user_id);
CREATE INDEX idx_subscriptions_feed ON subscriptions(feed_id);
CREATE INDEX idx_article_feeds_feed ON article_feeds(feed_id);
CREATE INDEX idx_read_states_user ON read_states(user_id);
CREATE INDEX idx_read_states_user_read ON read_states(user_id, is_read);
CREATE INDEX idx_articles_published ON articles(published_at);
CREATE INDEX idx_articles_fetched ON articles(fetched_at);
CREATE INDEX idx_articles_url ON articles(url);

CREATE VIRTUAL TABLE articles_fts USING fts5(
    title,
    summary,
    markdown_content,
    content='articles',
    content_rowid='id'
);

CREATE TRIGGER articles_fts_insert AFTER INSERT ON articles BEGIN
    INSERT INTO articles_fts(rowid, title, summary, markdown_content)
    VALUES (new.id, new.title, new.summary, new.markdown_content);
END;

CREATE TRIGGER articles_fts_delete AFTER DELETE ON articles BEGIN
    INSERT INTO articles_fts(articles_fts, rowid, title, summary, markdown_content)
    VALUES ('delete', old.id, old.title, old.summary, old.markdown_content);
END;

CREATE TRIGGER articles_fts_update AFTER UPDATE ON articles BEGIN
    INSERT INTO articles_fts(articles_fts, rowid, title, summary, markdown_content)
    VALUES ('delete', old.id, old.title, old.summary, old.markdown_content);
    INSERT INTO articles_fts(rowid, title, summary, markdown_content)
    VALUES (new.id, new.title, new.summary, new.markdown_content);
END;
