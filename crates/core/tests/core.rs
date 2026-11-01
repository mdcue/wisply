//! Tests for the database layout and the one-time import from the old Python app.

use std::path::Path;

use rusqlite::Connection;
use wisply_core::{legacy, schema, Error};

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |row| row.get(0)).unwrap()
}

/// Builds a database shaped like the old Python app's `data.db`.
fn make_legacy(path: &Path, with_notes: bool) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "
        CREATE TABLE bots (
            id TEXT PRIMARY KEY, name TEXT, color TEXT, active INTEGER DEFAULT 0,
            icon_path TEXT, expression TEXT DEFAULT 'smiling',
            fetch_interval_seconds INTEGER DEFAULT 3600, last_fetched_at TEXT
        );
        CREATE TABLE channels (
            id TEXT PRIMARY KEY, title TEXT, description TEXT, color TEXT,
            bot_id TEXT, position INTEGER DEFAULT 0
        );
        CREATE TABLE items (
            id INTEGER PRIMARY KEY AUTOINCREMENT, channel_id TEXT, text TEXT, timestamp TEXT,
            read INTEGER DEFAULT 0, url TEXT, price REAL, discount INTEGER, dismissed INTEGER DEFAULT 0
        );
        INSERT INTO bots (id, name, color, active, expression, fetch_interval_seconds, last_fetched_at) VALUES
            ('game_deals', 'Game Deals', '#ff8800', 1, 'happy', 7200, '2026-10-01 10:00:00'),
            ('github', 'GitHub', '#222222', 0, NULL, NULL, NULL);
        INSERT INTO channels (id, title, description, color, bot_id, position) VALUES
            ('deals_hot', 'Hot deals', 'Big discounts', '#ff0000', 'game_deals', 1),
            ('gh_trending', 'Trending', 'Popular repos', '#00ff00', 'github', 0),
            ('orphan_channel', 'Lost', '', '#000000', 'gone_bot', 0);
        INSERT INTO items (id, channel_id, text, timestamp, read, url, price, discount, dismissed) VALUES
            (1, 'deals_hot', 'Game A', 'key-a', 0, 'https://a.example', 9.99, 50, 0),
            (2, 'deals_hot', 'Game B', 'key-b', 1, NULL, NULL, 75, 1),
            (3, 'gh_trending', 'owner/repo', 'key-c', 0, 'https://github.com/owner/repo', NULL, 1234, 0),
            (4, 'deals_hot', 'Game A again', 'key-a', 0, NULL, NULL, NULL, 0),
            (5, 'no_such_channel', 'Stray', 'key-e', 0, NULL, NULL, NULL, 0),
            (6, 'orphan_channel', 'Stray 2', 'key-f', 0, NULL, NULL, NULL, 0),
            (7, 'deals_hot', 'Old item', NULL, 0, NULL, NULL, NULL, 0);
        ",
    )
    .unwrap();
    if with_notes {
        conn.execute_batch(
            "
            CREATE TABLE companion_notes (
                id INTEGER PRIMARY KEY AUTOINCREMENT, kind TEXT NOT NULL, text TEXT NOT NULL,
                due_at TEXT, repeat_rule TEXT, done INTEGER DEFAULT 0, last_shown TEXT, created_at TEXT
            );
            INSERT INTO companion_notes (id, kind, text, due_at, repeat_rule, done, created_at) VALUES
                (5, 'reminder', 'call mum', '2026-10-25 17:00', 'weekly', 0, '2026-10-20 09:00'),
                (9, 'idea', 'a wisp for weather', NULL, NULL, 0, NULL);
            ",
        )
        .unwrap();
    }
}

#[test]
fn fresh_database_is_at_latest_version_with_all_tables() {
    let conn = schema::open_in_memory().unwrap();
    assert_eq!(schema::version(&conn).unwrap(), schema::SCHEMA_VERSION);
    for table in ["wisps", "channels", "items", "companion_notes"] {
        let n = count(&conn, &format!("SELECT COUNT(*) FROM sqlite_master WHERE name = '{table}'"));
        assert_eq!(n, 1, "table {table} missing");
    }
}

#[test]
fn migrate_twice_changes_nothing() {
    let conn = schema::open_in_memory().unwrap();
    schema::migrate(&conn).unwrap();
    assert_eq!(schema::version(&conn).unwrap(), schema::SCHEMA_VERSION);
}

#[test]
fn newer_database_is_refused_and_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wisply.db");
    {
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("PRAGMA user_version = 99").unwrap();
    }
    match schema::open(&path) {
        Err(Error::NewerDatabase { found: 99, .. }) => {}
        other => panic!("expected NewerDatabase, got {:?}", other.map(|_| ())),
    }
    let conn = Connection::open(&path).unwrap();
    assert_eq!(count(&conn, "PRAGMA user_version"), 99);
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'"), 0);
}

#[test]
fn foreign_keys_are_enforced() {
    let conn = schema::open_in_memory().unwrap();
    let result = conn.execute(
        "INSERT INTO channels (id, wisp_id, title, color) VALUES ('c', 'nope', 't', '#000')",
        [],
    );
    assert!(result.is_err());
}

#[test]
fn import_maps_everything_correctly() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("data.db");
    make_legacy(&old, true);
    let before = std::fs::read(&old).unwrap();

    let conn = schema::open_in_memory().unwrap();
    let report = legacy::import(&old, &conn).unwrap();

    assert_eq!(
        report,
        legacy::Report {
            wisps: 2,
            channels: 2,
            items: 4,
            notes: 2,
            skipped_channels: 1,
            skipped_items: 2,
            skipped_duplicates: 1,
        }
    );

    // wisps: values and order kept, missing values get defaults.
    let (name, active, expr, interval, last): (String, i64, String, i64, Option<String>) = conn
        .query_row(
            "SELECT name, active, expression, fetch_interval_seconds, last_fetched_at FROM wisps WHERE id = 'game_deals'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!((name.as_str(), active, expr.as_str(), interval), ("Game Deals", 1, "happy", 7200));
    assert_eq!(last.as_deref(), Some("2026-10-01 10:00:00"));
    let (expr, interval): (String, i64) = conn
        .query_row("SELECT expression, fetch_interval_seconds FROM wisps WHERE id = 'github'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!((expr.as_str(), interval), ("smiling", 3600));
    assert_eq!(count(&conn, "SELECT position FROM wisps WHERE id = 'github'"), 1);

    // channels: linked to wisps.
    let wisp: String = conn.query_row("SELECT wisp_id FROM channels WHERE id = 'gh_trending'", [], |r| r.get(0)).unwrap();
    assert_eq!(wisp, "github");

    // items: ids kept, discount vs stars split, flags kept.
    let (pct, stars): (Option<i64>, Option<i64>) = conn
        .query_row("SELECT discount_percent, stars FROM items WHERE id = 1", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((pct, stars), (Some(50), None));
    let (pct, stars): (Option<i64>, Option<i64>) = conn
        .query_row("SELECT discount_percent, stars FROM items WHERE id = 3", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((pct, stars), (None, Some(1234)));
    let (read, dismissed): (i64, i64) = conn
        .query_row("SELECT read, dismissed FROM items WHERE id = 2", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert_eq!((read, dismissed), (1, 1));
    let key: String = conn.query_row("SELECT dedupe_key FROM items WHERE id = 7", [], |r| r.get(0)).unwrap();
    assert_eq!(key, "legacy:7");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM items WHERE id IN (4, 5, 6)"), 0);

    // notes: ids kept, missing values defaulted.
    let rule: String = conn.query_row("SELECT repeat_rule FROM companion_notes WHERE id = 9", [], |r| r.get(0)).unwrap();
    assert_eq!(rule, "none");
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM companion_notes WHERE id = 5"), 1);

    // nothing points at something missing, and the old file is untouched.
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM pragma_foreign_key_check"), 0);
    assert_eq!(std::fs::read(&old).unwrap(), before);
}

#[test]
fn second_import_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("data.db");
    make_legacy(&old, true);
    let conn = schema::open_in_memory().unwrap();
    legacy::import(&old, &conn).unwrap();
    assert!(matches!(legacy::import(&old, &conn), Err(Error::TargetNotEmpty)));
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM items"), 4);
}

#[test]
fn import_works_without_notes_table() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("data.db");
    make_legacy(&old, false);
    let conn = schema::open_in_memory().unwrap();
    let report = legacy::import(&old, &conn).unwrap();
    assert_eq!(report.notes, 0);
}

#[test]
fn import_works_on_an_old_database_missing_later_columns() {
    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("data.db");
    {
        let conn = Connection::open(&old).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE bots (id TEXT PRIMARY KEY, name TEXT, color TEXT);
            CREATE TABLE channels (id TEXT PRIMARY KEY, title TEXT, description TEXT, color TEXT, bot_id TEXT);
            CREATE TABLE items (id INTEGER PRIMARY KEY, channel_id TEXT, text TEXT, timestamp TEXT, read INTEGER);
            INSERT INTO bots VALUES ('b', 'B', '#111111');
            INSERT INTO channels VALUES ('c', 'C', 'd', '#222222', 'b');
            INSERT INTO items VALUES (1, 'c', 'hello', 'k1', 0);
            ",
        )
        .unwrap();
    }
    let conn = schema::open_in_memory().unwrap();
    let report = legacy::import(&old, &conn).unwrap();
    assert_eq!((report.wisps, report.channels, report.items), (1, 1, 1));
    assert_eq!(count(&conn, "SELECT fetch_interval_seconds FROM wisps"), 3600);
}

#[test]
fn missing_and_invalid_legacy_files_give_clear_errors() {
    let dir = tempfile::tempdir().unwrap();
    let conn = schema::open_in_memory().unwrap();
    assert!(matches!(
        legacy::import(&dir.path().join("nope.db"), &conn),
        Err(Error::LegacyMissing(_))
    ));
    let odd = dir.path().join("odd.db");
    Connection::open(&odd).unwrap().execute_batch("CREATE TABLE other (x)").unwrap();
    assert!(matches!(legacy::import(&odd, &conn), Err(Error::LegacyInvalid(_))));
}

#[test]
fn first_start_imports_once_and_later_starts_do_not() {
    let dir = tempfile::tempdir().unwrap();
    make_legacy(&dir.path().join("data.db"), true);

    let first = wisply_core::open_in_dir(dir.path()).unwrap();
    assert_eq!(first.imported.as_ref().map(|r| r.items), Some(4));
    drop(first);

    let second = wisply_core::open_in_dir(dir.path()).unwrap();
    assert!(second.imported.is_none());
    assert_eq!(count(&second.conn, "SELECT COUNT(*) FROM items"), 4);
}

#[test]
fn first_start_without_old_data_just_creates_an_empty_database() {
    let dir = tempfile::tempdir().unwrap();
    let opened = wisply_core::open_in_dir(dir.path()).unwrap();
    assert!(opened.imported.is_none());
    assert_eq!(count(&opened.conn, "SELECT COUNT(*) FROM wisps"), 0);
}