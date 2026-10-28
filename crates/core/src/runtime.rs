//! What the running app needs from the core, in one place: set up the wisps, start the scheduler,
//! and say how many things are unread. The app (`src-tauri`) only calls these, so everything that
//! can be tested without a window is tested here.

use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::scheduler::{self, Fetcher, Handle};
use crate::wisps::companion::{self, CompanionFetcher};
use crate::wisps::deals::{self, DealsFetcher};
use crate::wisps::github::{self, GithubFetcher};
use crate::{paths, schema, store, Result};

/// The face names that exist (the pictures built into the program, see `faces`).
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

/// The text for the tray icon's hover tip: how many unread items the switched-on wisps have.
pub fn tooltip(conn: &Connection) -> Result<String> {
    let mut unread = 0;
    for wisp in store::active_wisps(conn)? {
        unread += store::wisp_unread(conn, &wisp.id)?;
    }
    Ok(if unread == 0 { "Wisply".to_string() } else { format!("Wisply: {unread} unread") })
}

/// Starts checking the wisps in the background (see `scheduler`). `on_update` gets the new hover
/// text each time a round of checks finishes. Keep the returned handle for as long as the app
/// runs: dropping it stops the scheduler.
pub fn start(data_dir: &Path, on_update: impl Fn(String) + Send + 'static) -> Handle {
    start_with(data_dir, fetchers(), scheduler::TICK, on_update)
}

/// Like `start`, with your own fetchers and tick length (used by tests).
pub fn start_with(
    data_dir: &Path,
    fetchers: Vec<Box<dyn Fetcher>>,
    tick: Duration,
    on_update: impl Fn(String) + Send + 'static,
) -> Handle {
    let db_path = data_dir.join(paths::DB_FILE);
    let for_text = db_path.clone();
    scheduler::spawn(db_path, fetchers, tick, move || match hover_text(&for_text) {
        Ok(text) => on_update(text),
        Err(error) => log::warn!("could not count unread items: {error}"),
    })
}

/// Opens the database for a moment and builds the hover text.
fn hover_text(db_path: &Path) -> Result<String> {
    tooltip(&schema::open(db_path)?)
}