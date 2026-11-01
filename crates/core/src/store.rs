//! Reading and writing wisps, channels and items. This is the new home of everything the Python
//! app's `core/db.py` did, with the same behaviour and proper types instead of tuples.
//!
//! Every function takes the connection as its first argument. All user-supplied values go in as
//! bound parameters (`?1`, `?2`...), never pasted into the SQL text, so they can't change the query.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::Result;

/// A wisp: one small single-purpose watcher (the old code called these "bots").
#[derive(Debug, Clone, PartialEq)]
pub struct Wisp {
    pub id: String,
    pub name: String,
    pub color: String,
    pub active: bool,
    pub expression: String,
    /// 0 means "only when the app starts".
    pub fetch_interval_seconds: i64,
    /// UTC, as written by SQLite's `datetime('now')`.
    pub last_fetched_at: Option<String>,
    pub position: i64,
}

/// A channel: one feed a wisp fills (for example "Hot deals").
#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    pub id: String,
    pub title: String,
    pub description: String,
    pub color: String,
}

/// An item as the Hub lists it inside one channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: i64,
    pub text: String,
    pub read: bool,
    pub url: Option<String>,
    pub price: Option<f64>,
    pub discount_percent: Option<i64>,
    pub stars: Option<i64>,
}

/// An item in the mixed "latest from every wisp" list, with where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct FeedEntry {
    pub item_id: i64,
    pub text: String,
    pub read: bool,
    pub url: Option<String>,
    pub channel_id: String,
    pub wisp_id: String,
    pub wisp_name: String,
}

/// A new item a wisp has found. `dedupe_key` must be the same every time the same thing is found,
/// so it is only ever added once.
#[derive(Debug, Clone, Default)]
pub struct NewItem {
    pub channel_id: String,
    pub dedupe_key: String,
    pub text: String,
    pub url: Option<String>,
    pub price: Option<f64>,
    pub discount_percent: Option<i64>,
    pub stars: Option<i64>,
    pub extra_json: Option<String>,
}

/// Changes to a wisp's looks. Anything left as `None` stays as it was.
#[derive(Debug, Clone, Default)]
pub struct WispChanges {
    pub name: Option<String>,
    pub color: Option<String>,
    pub expression: Option<String>,
}

const WISP_COLUMNS: &str =
    "id, name, color, active, expression, fetch_interval_seconds, last_fetched_at, position";

fn wisp_from_row(row: &Row) -> rusqlite::Result<Wisp> {
    Ok(Wisp {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        active: row.get::<_, i64>(3)? != 0,
        expression: row.get(4)?,
        fetch_interval_seconds: row.get(5)?,
        last_fetched_at: row.get(6)?,
        position: row.get(7)?,
    })
}

// ---- wisps ----

/// Adds a wisp if it doesn't exist yet (switched off, placed last). Returns true if it was added.
pub fn add_wisp(conn: &Connection, id: &str, name: &str, color: &str) -> Result<bool> {
    let added = conn.execute(
        "INSERT OR IGNORE INTO wisps (id, name, color, active, position)
         VALUES (?1, ?2, ?3, 0, (SELECT COALESCE(MAX(position) + 1, 0) FROM wisps))",
        params![id, name, color],
    )?;
    Ok(added == 1)
}

pub fn get_wisp(conn: &Connection, id: &str) -> Result<Option<Wisp>> {
    let mut stmt = conn.prepare(&format!("SELECT {WISP_COLUMNS} FROM wisps WHERE id = ?1"))?;
    Ok(stmt.query_row([id], wisp_from_row).optional()?)
}

pub fn all_wisps(conn: &Connection) -> Result<Vec<Wisp>> {
    let mut stmt = conn.prepare(&format!("SELECT {WISP_COLUMNS} FROM wisps ORDER BY position, rowid"))?;
    let wisps = stmt.query_map([], wisp_from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(wisps)
}

pub fn active_wisps(conn: &Connection) -> Result<Vec<Wisp>> {
    let mut stmt = conn
        .prepare(&format!("SELECT {WISP_COLUMNS} FROM wisps WHERE active = 1 ORDER BY position, rowid"))?;
    let wisps = stmt.query_map([], wisp_from_row)?.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(wisps)
}

pub fn set_fetch_interval(conn: &Connection, id: &str, seconds: i64) -> Result<()> {
    conn.execute("UPDATE wisps SET fetch_interval_seconds = ?1 WHERE id = ?2", params![seconds, id])?;
    Ok(())
}

/// Records "just checked" (UTC, from the database clock).
pub fn mark_fetched(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE wisps SET last_fetched_at = datetime('now') WHERE id = ?1", [id])?;
    Ok(())
}

pub fn update_wisp(conn: &Connection, id: &str, changes: &WispChanges) -> Result<()> {
    conn.execute(
        "UPDATE wisps SET name = COALESCE(?1, name), color = COALESCE(?2, color),
                          expression = COALESCE(?3, expression)
         WHERE id = ?4",
        params![changes.name, changes.color, changes.expression, id],
    )?;
    Ok(())
}

pub fn set_wisp_active(conn: &Connection, id: &str, active: bool) -> Result<()> {
    conn.execute("UPDATE wisps SET active = ?1 WHERE id = ?2", params![active as i64, id])?;
    Ok(())
}

pub fn wisp_channels(conn: &Connection, wisp_id: &str) -> Result<Vec<Channel>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, description, color FROM channels WHERE wisp_id = ?1 ORDER BY position, rowid",
    )?;
    let channels = stmt
        .query_map([wisp_id], |row| {
            Ok(Channel { id: row.get(0)?, title: row.get(1)?, description: row.get(2)?, color: row.get(3)? })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(channels)
}

/// Total unread items across all of a wisp's channels.
pub fn wisp_unread(conn: &Connection, wisp_id: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM items JOIN channels ON items.channel_id = channels.id
         WHERE channels.wisp_id = ?1 AND items.read = 0",
        [wisp_id],
        |row| row.get(0),
    )?)
}

// ---- channels ----

/// Adds a channel, or updates its title, description, wisp and position if it already exists.
/// (Its colour is kept, as before, so a colour the user picked isn't reset.)
pub fn upsert_channel(
    conn: &Connection,
    id: &str,
    wisp_id: &str,
    title: &str,
    description: &str,
    color: &str,
    position: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO channels (id, wisp_id, title, description, color, position)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(id) DO UPDATE SET
             title = excluded.title,
             description = excluded.description,
             wisp_id = excluded.wisp_id,
             position = excluded.position",
        params![id, wisp_id, title, description, color, position],
    )?;
    Ok(())
}

// ---- items ----

/// Adds an item unless one with the same `dedupe_key` already exists (in any channel).
/// Returns true if it was added.
pub fn add_item(conn: &Connection, item: &NewItem) -> Result<bool> {
    let added = conn.execute(
        "INSERT OR IGNORE INTO items
            (channel_id, dedupe_key, text, url, price, discount_percent, stars, extra_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            item.channel_id,
            item.dedupe_key,
            item.text,
            item.url,
            item.price,
            item.discount_percent,
            item.stars,
            item.extra_json,
        ],
    )?;
    Ok(added == 1)
}

/// True if an item with this duplicate-check key is already stored, in any channel.
pub fn has_item(conn: &Connection, dedupe_key: &str) -> Result<bool> {
    let found: Option<i64> = conn
        .query_row("SELECT 1 FROM items WHERE dedupe_key = ?1 LIMIT 1", [dedupe_key], |row| row.get(0))
        .optional()?;
    Ok(found.is_some())
}

pub fn unread_count(conn: &Connection, channel_id: &str) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM items WHERE channel_id = ?1 AND read = 0 AND dismissed = 0",
        [channel_id],
        |row| row.get(0),
    )?)
}

/// Unread items per channel, for every channel that has any, in one query.
pub fn unread_counts(conn: &Connection) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare(
        "SELECT channel_id, COUNT(*) FROM items WHERE read = 0 AND dismissed = 0 GROUP BY channel_id",
    )?;
    let pairs = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<std::result::Result<HashMap<_, _>, _>>()?;
    Ok(pairs)
}

/// The number of the newest item ever saved (0 if there are none). Item numbers only go up, so
/// "anything above the number I saw last time" means "new since then".
pub fn newest_item_id(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COALESCE(MAX(id), 0) FROM items", [], |row| row.get(0))?)
}

/// For each switched-on wisp that has unread items newer than `after_id` (see `newest_item_id`):
/// its name and how many such items, in the wisps' usual order. This decides who gets a pop-up.
pub fn wisps_with_news_since(conn: &Connection, after_id: i64) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT wisps.name, COUNT(items.id) FROM wisps
        JOIN channels ON channels.wisp_id = wisps.id
        JOIN items ON items.channel_id = channels.id
        WHERE wisps.active = 1 AND items.id > ?1 AND items.read = 0 AND items.dismissed = 0
        GROUP BY wisps.id
        ORDER BY wisps.position, wisps.id",
    )?;
    let news = stmt
        .query_map([after_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(news)
}

/// A channel's items, newest first. `limit` caps how many come back (`None` means all).
pub fn items(conn: &Connection, channel_id: &str, limit: Option<i64>) -> Result<Vec<Item>> {
    let mut stmt = conn.prepare(
        "SELECT id, text, read, url, price, discount_percent, stars FROM items
        WHERE channel_id = ?1 AND dismissed = 0 ORDER BY id DESC LIMIT ?2",
    )?;
    let items = stmt
        .query_map(params![channel_id, limit.unwrap_or(-1)], |row| {
            Ok(Item {
                id: row.get(0)?,
                text: row.get(1)?,
                read: row.get::<_, i64>(2)? != 0,
                url: row.get(3)?,
                price: row.get(4)?,
                discount_percent: row.get(5)?,
                stars: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(items)
}

/// The newest items across every active wisp, newest first, each with the wisp it came from.
pub fn recent_items(conn: &Connection, limit: i64) -> Result<Vec<FeedEntry>> {
    let mut stmt = conn.prepare(
        "SELECT items.id, items.text, items.read, items.url, items.channel_id, wisps.id, wisps.name
        FROM items
        JOIN channels ON items.channel_id = channels.id
        JOIN wisps ON channels.wisp_id = wisps.id
        WHERE wisps.active = 1 AND items.dismissed = 0
        ORDER BY items.id DESC LIMIT ?1",
    )?;
    let entries = stmt
        .query_map([limit], |row| {
            Ok(FeedEntry {
                item_id: row.get(0)?,
                text: row.get(1)?,
                read: row.get::<_, i64>(2)? != 0,
                url: row.get(3)?,
                channel_id: row.get(4)?,
                wisp_id: row.get(5)?,
                wisp_name: row.get(6)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(entries)
}

/// The link stored with an item, if it has one.
pub fn item_url(conn: &Connection, item_id: i64) -> Result<Option<String>> {
    let url: Option<Option<String>> =
        conn.query_row("SELECT url FROM items WHERE id = ?1", [item_id], |row| row.get(0)).optional()?;
    Ok(url.flatten())
}

pub fn mark_read(conn: &Connection, item_id: i64) -> Result<()> {
    conn.execute("UPDATE items SET read = 1 WHERE id = ?1", [item_id])?;
    Ok(())
}

pub fn mark_all_read(conn: &Connection, channel_id: &str) -> Result<()> {
    conn.execute("UPDATE items SET read = 1 WHERE channel_id = ?1 AND read = 0", [channel_id])?;
    Ok(())
}

/// Removes an item from every list for good, and marks it read. The row stays, with its
/// duplicate-check key, so the same thing is never added again.
pub fn dismiss_item(conn: &Connection, item_id: i64) -> Result<()> {
    conn.execute("UPDATE items SET read = 1, dismissed = 1 WHERE id = ?1", [item_id])?;
    Ok(())
}

/// Removes every read item in a channel from view (see `dismiss_item`).
pub fn dismiss_read(conn: &Connection, channel_id: &str) -> Result<()> {
    conn.execute("UPDATE items SET dismissed = 1 WHERE channel_id = ?1 AND read = 1", [channel_id])?;
    Ok(())
}