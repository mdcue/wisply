//! The database layout ("data format v2") and the way it changes over time.
//!
//! SQLite keeps a number called `user_version` inside the file. Each entry in `MIGRATIONS` upgrades
//! the file by one version, and the number is bumped in the same transaction, so a file is never
//! left half upgraded. To change the layout later, add one more entry; never edit an old one.

use std::path::Path;

use rusqlite::Connection;

use crate::{Error, Result};

/// Migration number `n` (counting from 0) upgrades a database from version `n` to `n + 1`.
const MIGRATIONS: &[&str] = &[
    // 0 -> 1: the first layout of the new app.
    "
    CREATE TABLE wisps (
        id                      TEXT PRIMARY KEY,
        name                    TEXT NOT NULL,
        color                   TEXT NOT NULL,
        active                  INTEGER NOT NULL DEFAULT 0,
        expression              TEXT NOT NULL DEFAULT 'smiling',
        fetch_interval_seconds  INTEGER NOT NULL DEFAULT 3600,
        last_fetched_at         TEXT,
        position                INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE channels (
        id           TEXT PRIMARY KEY,
        wisp_id      TEXT NOT NULL REFERENCES wisps(id) ON DELETE CASCADE,
        title        TEXT NOT NULL,
        description  TEXT NOT NULL DEFAULT '',
        color        TEXT NOT NULL,
        position     INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE items (
        id                INTEGER PRIMARY KEY AUTOINCREMENT,
        channel_id        TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
        dedupe_key        TEXT NOT NULL,
        text              TEXT NOT NULL,
        url               TEXT,
        price             REAL,
        discount_percent  INTEGER,
        stars             INTEGER,
        extra_json        TEXT,
        read              INTEGER NOT NULL DEFAULT 0,
        dismissed         INTEGER NOT NULL DEFAULT 0,
        created_at        TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE UNIQUE INDEX idx_items_dedupe ON items(dedupe_key);
    CREATE INDEX idx_items_channel ON items(channel_id, id);
    CREATE INDEX idx_items_unread ON items(channel_id, read);
    CREATE INDEX idx_channels_wisp ON channels(wisp_id);

    CREATE TABLE companion_notes (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        kind         TEXT NOT NULL,
        text         TEXT NOT NULL,
        due_at       TEXT,
        repeat_rule  TEXT NOT NULL DEFAULT 'none',
        done         INTEGER NOT NULL DEFAULT 0,
        last_shown   TEXT,
        created_at   TEXT NOT NULL
    );
    ",
];

/// The newest data format this version of Wisply understands.
pub const SCHEMA_VERSION: i32 = MIGRATIONS.len() as i32;

/// Opens (and creates, and upgrades) the database at `path`.
pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    prepare(&conn)?;
    Ok(conn)
}

/// An in-memory database, for tests.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    prepare(&conn)?;
    Ok(conn)
}

fn prepare(conn: &Connection) -> Result<()> {
    // Wait up to 5 seconds instead of failing at once when another part of the app is writing.
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    // SQLite does not enforce links between tables unless asked to, once per connection.
    conn.pragma_update(None, "foreign_keys", "ON")?;
    // Lets the scheduler write while the Hub reads. (Quietly stays as it was for in-memory databases.)
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrate(conn)
}

/// The data format version stored in the file (0 for a brand new file).
pub fn version(conn: &Connection) -> Result<i32> {
    Ok(conn.query_row("PRAGMA user_version", [], |row| row.get(0))?)
}

/// Brings the database up to `SCHEMA_VERSION`. Safe to call every time the app starts.
pub fn migrate(conn: &Connection) -> Result<()> {
    let current = version(conn)?;
    if current > SCHEMA_VERSION {
        return Err(Error::NewerDatabase { found: current, supported: SCHEMA_VERSION });
    }
    for step in current..SCHEMA_VERSION {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(MIGRATIONS[step as usize])?;
        // PRAGMA does not accept a bound parameter, and `step` is our own number, never user input.
        tx.execute_batch(&format!("PRAGMA user_version = {}", step + 1))?;
        tx.commit()?;
    }
    Ok(())
}