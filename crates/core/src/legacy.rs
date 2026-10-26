//! One-time import of the old Python app's `data.db` into the new database.
//!
//! The old file is opened read only and is never changed. Everything is imported inside a single
//! transaction, so either all of it arrives or none of it does.
//!
//! What changes on the way in:
//! * `bots` becomes `wisps`; the icon path is dropped (the new app works it out from the wisp's id).
//! * `items.timestamp` (which always held the duplicate-check key) becomes `dedupe_key`.
//! * `items.discount` meant "percent off" for deals but "star count" for GitHub, so GitHub's
//!   (channels starting with `gh_`) moves to the new `stars` column.
//! * Item ids are kept, so "newest first" stays exactly as it was.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags};

use crate::{Error, Result};

/// What the import did, in plain numbers.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub wisps: usize,
    pub channels: usize,
    pub items: usize,
    pub notes: usize,
    /// Channels whose wisp no longer exists.
    pub skipped_channels: usize,
    /// Items whose channel doesn't exist (or was skipped).
    pub skipped_items: usize,
    /// Items whose duplicate-check key was already used by an earlier item.
    pub skipped_duplicates: usize,
}

/// Imports from the file at `legacy_path` into `target`, which must not contain any wisps yet.
pub fn import(legacy_path: &Path, target: &Connection) -> Result<Report> {
    if !legacy_path.exists() {
        return Err(Error::LegacyMissing(legacy_path.to_path_buf()));
    }
    let old = Connection::open_with_flags(legacy_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    for table in ["bots", "channels", "items"] {
        if !table_exists(&old, table)? {
            return Err(Error::LegacyInvalid(format!("the table `{table}` is missing")));
        }
    }

    let already: i64 = target.query_row("SELECT COUNT(*) FROM wisps", [], |row| row.get(0))?;
    if already > 0 {
        return Err(Error::TargetNotEmpty);
    }

    let tx = target.unchecked_transaction()?;
    let mut report = Report::default();

    // Old databases may lack columns that were added later, so only ask for what exists.
    let bot_cols = columns(&old, "bots")?;
    let wisp_ids = import_wisps(&old, &tx, &bot_cols, &mut report)?;
    let channel_cols = columns(&old, "channels")?;
    let channel_ids = import_channels(&old, &tx, &channel_cols, &wisp_ids, &mut report)?;
    let item_cols = columns(&old, "items")?;
    import_items(&old, &tx, &item_cols, &channel_ids, &mut report)?;
    if table_exists(&old, "companion_notes")? {
        import_notes(&old, &tx, &mut report)?;
    }

    tx.commit()?;
    Ok(report)
}

fn table_exists(conn: &Connection, name: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn columns(conn: &Connection, table: &str) -> Result<HashSet<String>> {
    // `table` is always one of our own fixed names, never user input.
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<std::result::Result<HashSet<_>, _>>()?;
    Ok(names)
}

/// The column's name if it exists, otherwise a fixed fallback value (so the query still works).
fn col(cols: &HashSet<String>, name: &str, fallback: &str) -> String {
    if cols.contains(name) {
        name.to_string()
    } else {
        fallback.to_string()
    }
}

fn import_wisps(
    old: &Connection,
    tx: &Connection,
    cols: &HashSet<String>,
    report: &mut Report,
) -> Result<HashSet<String>> {
    let sql = format!(
        "SELECT id, COALESCE(name, id), COALESCE(color, '#888888'), {}, {}, {}, {} FROM bots ORDER BY rowid",
        col(cols, "active", "1"),
        col(cols, "expression", "'smiling'"),
        col(cols, "fetch_interval_seconds", "3600"),
        col(cols, "last_fetched_at", "NULL"),
    );
    let mut stmt = old.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut ids = HashSet::new();
    let mut position = 0;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        tx.execute(
            "INSERT INTO wisps (id, name, color, active, expression, fetch_interval_seconds, last_fetched_at, position)
             VALUES (?1, ?2, ?3, ?4, COALESCE(?5, 'smiling'), COALESCE(?6, 3600), ?7, ?8)",
            params![
                id,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<i64>>(3)?.unwrap_or(1),
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<String>>(6)?,
                position,
            ],
        )?;
        ids.insert(id);
        position += 1;
        report.wisps += 1;
    }
    Ok(ids)
}

fn import_channels(
    old: &Connection,
    tx: &Connection,
    cols: &HashSet<String>,
    wisp_ids: &HashSet<String>,
    report: &mut Report,
) -> Result<HashSet<String>> {
    let sql = format!(
        "SELECT id, COALESCE(title, id), COALESCE(description, ''), COALESCE(color, '#888888'), {}, {} FROM channels ORDER BY rowid",
        col(cols, "bot_id", "NULL"),
        col(cols, "position", "0"),
    );
    let mut stmt = old.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut ids = HashSet::new();
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let wisp_id: Option<String> = row.get(4)?;
        let Some(wisp_id) = wisp_id.filter(|w| wisp_ids.contains(w)) else {
            report.skipped_channels += 1;
            continue;
        };
        tx.execute(
            "INSERT INTO channels (id, wisp_id, title, description, color, position) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id,
                wisp_id,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            ],
        )?;
        ids.insert(id);
        report.channels += 1;
    }
    Ok(ids)
}

fn import_items(
    old: &Connection,
    tx: &Connection,
    cols: &HashSet<String>,
    channel_ids: &HashSet<String>,
    report: &mut Report,
) -> Result<()> {
    let sql = format!(
        "SELECT id, channel_id, COALESCE(text, ''), timestamp, COALESCE(read, 0), {}, {}, {}, COALESCE({}, 0) FROM items ORDER BY id",
        col(cols, "url", "NULL"),
        col(cols, "price", "NULL"),
        col(cols, "discount", "NULL"),
        col(cols, "dismissed", "0"),
    );
    let mut stmt = old.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let channel_id: Option<String> = row.get(1)?;
        let Some(channel_id) = channel_id.filter(|c| channel_ids.contains(c)) else {
            report.skipped_items += 1;
            continue;
        };
        // Very old items may have no key at all; give them a unique one so they still import.
        let key: String = row.get::<_, Option<String>>(3)?.unwrap_or_else(|| format!("legacy:{id}"));
        let number: Option<i64> = row.get(7)?;
        let (discount_percent, stars) =
            if channel_id.starts_with("gh_") { (None, number) } else { (number, None) };

        let inserted = tx.execute(
            "INSERT OR IGNORE INTO items
                (id, channel_id, dedupe_key, text, url, price, discount_percent, stars, read, dismissed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                channel_id,
                key,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<f64>>(6)?,
                discount_percent,
                stars,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(8)?,
            ],
        )?;
        if inserted == 0 {
            report.skipped_duplicates += 1;
        } else {
            report.items += 1;
        }
    }
    Ok(())
}

fn import_notes(old: &Connection, tx: &Connection, report: &mut Report) -> Result<()> {
    let mut stmt = old.prepare(
        "SELECT id, kind, text, due_at, COALESCE(repeat_rule, 'none'), COALESCE(done, 0), last_shown,
                COALESCE(created_at, datetime('now'))
         FROM companion_notes ORDER BY id",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        tx.execute(
            "INSERT INTO companion_notes (id, kind, text, due_at, repeat_rule, done, last_shown, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
            ],
        )?;
        report.notes += 1;
    }
    Ok(())
}