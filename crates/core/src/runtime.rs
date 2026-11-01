//! What the running app needs from the core, in one place: set up the wisps, start the scheduler,
//! and say how many things are unread. The app (`src-tauri`) only calls these, so everything that
//! can be tested without a window is tested here.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::Connection;

use crate::scheduler::{self, Fetcher, Handle};
use crate::wisps::companion::{self, CompanionFetcher};
use crate::wisps::deals::{self, DealsFetcher};
use crate::wisps::github::{self, GithubFetcher};
use crate::{paths, schema, store, Result};

/// The face names that exist (the pictures built into the program, see `faces`). Companion picks
/// its mood face from these.
pub const FACES: &[&str] = crate::faces::NAMES;

/// Opens (and if needed creates, upgrades or imports) the database in `data_dir`, and sets up the
/// three wisps. Returns what was imported from the old Python app, if this was the first start.
/// Call this once when the app starts, before `start`.
pub fn prepare(data_dir: &Path) -> Result<Option<crate::legacy::Report>> {
    let opened = crate::open_in_dir(data_dir)?;
    if let Some(report) = &opened.imported {
        log::info!(
            "imported {} wisps, {} channels, {} items and {} notes from the old app",
            report.wisps,
            report.channels,
            report.items,
            report.notes
        );
    }
    register_wisps(&opened.conn)?;
    Ok(opened.imported)
}

/// Adds the three wisps and their channels. Safe to call on every start: a wisp that already
/// exists keeps everything the user changed (on/off, colour, interval).
pub fn register_wisps(conn: &Connection) -> Result<()> {
    deals::register_wisp(conn)?;
    github::register_wisp(conn)?;
    companion::register_wisp(conn)?;
    Ok(())
}

/// One fetcher for each wisp, ready for the scheduler.
pub fn fetchers() -> Vec<Box<dyn Fetcher>> {
    vec![
        Box::new(DealsFetcher::new()),
        Box::new(GithubFetcher::new()),
        Box::new(CompanionFetcher::new().with_faces(FACES.iter().map(|f| f.to_string()).collect())),
    ]
}

/// The fetcher for one wisp, if there is such a wisp.
pub fn fetcher_for(wisp_id: &str) -> Option<Box<dyn Fetcher>> {
    fetchers().into_iter().find(|f| f.wisp_id() == wisp_id)
}

/// The text for the tray icon's hover tip: how many unread items the switched-on wisps have.
pub fn tooltip(conn: &Connection) -> Result<String> {
    Ok(status(conn)?.tooltip)
}

/// What the tray needs to show after each round of checks.
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    /// The hover tip: "Wisply" or "Wisply: N unread" (switched-on wisps only).
    pub tooltip: String,
    /// Whether the tray icon should wear the red dot.
    pub has_unread: bool,
    /// The switched-on wisps that found something new since the last update, with how many new
    /// items each found; each one gets a pop-up.
    pub new_from: Vec<(String, i64)>,
}

/// The tooltip and badge as things stand now (nothing counts as new).
pub fn status(conn: &Connection) -> Result<Update> {
    let mut unread = 0;
    for wisp in store::active_wisps(conn)? {
        unread += store::wisp_unread(conn, &wisp.id)?;
    }
    let tooltip = if unread == 0 { "Wisply".to_string() } else { format!("Wisply: {unread} unread") };
    Ok(Update { tooltip, has_unread: unread > 0, new_from: Vec::new() })
}

/// `status` for the database in `data_dir` (used when the app starts, to show the right icon).
pub fn status_in(data_dir: &Path) -> Result<Update> {
    status(&schema::open(&data_dir.join(paths::DB_FILE))?)
}

/// Starts checking the wisps in the background (see `scheduler`). `on_update` is called each time
/// a round of checks finishes, with what the tray should show. Keep the returned handle for as
/// long as the app runs: dropping it stops the scheduler. After each round the wisps' icons are
/// redrawn if a colour or face changed (for example when pip's mood changes).
pub fn start(data_dir: &Path, on_update: impl Fn(Update) + Send + 'static) -> Handle {
    let dir = data_dir.to_path_buf();
    start_with(data_dir, fetchers(), scheduler::TICK, move |update| {
        refresh_icons(&dir);
        on_update(update);
    })
}

/// Redraws any wisp icon that is missing or out of date (see `appearance::ensure_icons`). Problems
/// are logged, never fatal.
pub fn refresh_icons(data_dir: &Path) {
    let result = schema::open(&data_dir.join(paths::DB_FILE))
        .and_then(|conn| crate::appearance::ensure_icons(&conn, data_dir));
    if let Err(error) = result {
        log::warn!("could not refresh the wisp icons: {error}");
    }
}

/// Like `start`, with your own fetchers and tick length, and without redrawing icons (used by tests).
pub fn start_with(
    data_dir: &Path,
    fetchers: Vec<Box<dyn Fetcher>>,
    tick: Duration,
    on_update: impl Fn(Update) + Send + 'static,
) -> Handle {
    let db_path = data_dir.join(paths::DB_FILE);
    // Items up to this number were already there when the app started; only newer ones are "new".
    let seen = Mutex::new(schema::open(&db_path).and_then(|c| store::newest_item_id(&c)).unwrap_or(0));
    scheduler::spawn(db_path.clone(), fetchers, tick, move || match build_update(&db_path, &seen) {
        Ok(update) => on_update(update),
        Err(error) => log::warn!("could not count unread items: {error}"),
    })
}

/// Opens the database for a moment, works out what is new since last time, and builds the update.
fn build_update(db_path: &Path, seen: &Mutex<i64>) -> Result<Update> {
    let conn = schema::open(db_path)?;
    let mut update = status(&conn)?;
    let mut last = seen.lock().unwrap_or_else(|e| e.into_inner());
    update.new_from = store::wisps_with_news_since(&conn, *last)?;
    *last = store::newest_item_id(&conn)?;
    Ok(update)
}
