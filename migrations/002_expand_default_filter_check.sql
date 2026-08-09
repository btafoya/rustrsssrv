-- SQLite requires recreating the table to broaden a CHECK constraint.
PRAGMA foreign_keys = OFF;

CREATE TABLE users_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    default_filter TEXT NOT NULL DEFAULT 'unread'
        CHECK (default_filter IN ('all', 'unread', 'read', 'starred')),
    default_sort_order TEXT NOT NULL DEFAULT 'oldest_first'
        CHECK (default_sort_order IN ('oldest_first', 'newest_first')),
    created_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s','now') * 1000)
);

INSERT INTO users_new SELECT * FROM users;

DROP TABLE users;
ALTER TABLE users_new RENAME TO users;

PRAGMA foreign_keys = ON;
