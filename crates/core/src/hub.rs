//! Everything the Hub page asks for, as one typed thing. This is the new home of the Python app's
//! `ui/bridge.py`: the app's window code only passes the page's requests on to `Hub`, so all of it
//! can be tested without a window. The answers are plain structs that turn into the same JSON the
//! page already reads.
//!
//! Each call opens the database for a moment (cheap, and it means calls from any thread are safe).

use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDateTime, TimeZone};
use rusqlite::Connection;
use serde::Serialize;

use crate::scheduler::Fetcher;
use crate::wisps::companion::{self, Feeling, Idea, NoteError, Reminder};
use crate::wisps::companion_voice as voice;
use crate::wisps::{deals, github};
use crate::{account, appearance, faces, icons, paths, runtime, safe, schema, store, Result};

/// The check intervals offered for wisps that don't have their own list (0 = only at start-up).
pub const DEFAULT_INTERVALS: [i64; 6] = [0, 900, 1800, 3600, 7200, 14400];
/// The longest name a wisp can be given.
pub const MAX_NAME_CHARS: usize = 40;
/// Items shown per channel in the Hub.
const ITEMS_PER_CHANNEL: i64 = 10;
/// Items shown in the "latest from every wisp" list.
const FEED_LENGTH: i64 = 30;
/// What the page is told when a note couldn't be saved for a reason that isn't the user's fault.
const TROUBLE: &str = "Something went wrong saving that. Please try again.";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WispView {
    pub id: String,
    pub name: String,
    pub color: String,
    pub active: bool,
    /// The wisp's icon as a data address the page can show directly (empty if there is none yet).
    pub icon: String,
    pub expression: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SettingsWisp {
    pub id: String,
    pub name: String,
    pub color: String,
    pub icon: String,
    pub fetch_interval: i64,
    pub intervals: Vec<i64>,
    /// In the person's own time zone, or empty if it has never been checked.
    pub last_fetched: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemView {
    pub id: i64,
    pub text: String,
    pub read: bool,
    pub url: String,
    pub price: Option<f64>,
    pub discount: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChannelView {
    pub id: String,
    pub title: String,
    pub color: String,
    pub unread: i64,
    pub items: Vec<ItemView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FeedItem {
    pub item_id: i64,
    pub text: String,
    pub read: bool,
    pub url: String,
    pub wisp_name: String,
    pub wisp_icon: String,
    pub channel_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotesView {
    pub reminders: Vec<Reminder>,
    pub ideas: Vec<Idea>,
    pub empty_reminders: &'static str,
    pub empty_ideas: &'static str,
}

/// The answer to adding, changing or removing a note: it worked (with Companion's cheerful
/// confirmation, if there is one), or here is a sentence to show.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChangeResult {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub message: Option<String>,
}

/// How a manual or first check went.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchReport {
    pub wisp_id: String,
    pub ok: bool,
    /// How many more unread items the wisp has than before the check (never below 0).
    pub new_items: i64,
}

/// The Hub's view of the app's data. Cheap to make; keep one for the life of the app.
pub struct Hub {
    dir: PathBuf,
    clock: Box<dyn Fn() -> NaiveDateTime + Send + Sync>,
}

impl Hub {
    pub fn new(data_dir: &Path) -> Hub {
        Hub { dir: data_dir.to_path_buf(), clock: Box::new(|| Local::now().naive_local()) }
    }

    /// With your own idea of "now" (local wall-clock time), for tests.
    pub fn with_clock(mut self, clock: impl Fn() -> NaiveDateTime + Send + Sync + 'static) -> Hub {
        self.clock = Box::new(clock);
        self
    }

    /// The data folder this Hub reads from.
    pub fn data_dir(&self) -> &Path {
        &self.dir
    }

    fn conn(&self) -> Result<Connection> {
        schema::open(&self.dir.join(paths::DB_FILE))
    }

    fn now(&self) -> NaiveDateTime {
        (self.clock)()
    }

    // ---- account ----

    pub fn account_exists(&self) -> bool {
        account::exists(&self.dir)
    }

    /// Creates the account. `false` if the name is empty or the file couldn't be written.
    pub fn create_account(&self, name: &str) -> bool {
        match account::create(&self.dir, name) {
            Ok(_) => true,
            Err(error) => {
                if !name.trim().is_empty() {
                    log::warn!("could not create the account: {error}");
                }
                false
            }
        }
    }

    // ---- wisps ----

    /// The faces a wisp can wear.
    pub fn expressions(&self) -> Vec<&'static str> {
        faces::NAMES.to_vec()
    }

    pub fn wisps(&self) -> Result<Vec<WispView>> {
        let conn = self.conn()?;
        Ok(store::all_wisps(&conn)?
            .into_iter()
            .map(|w| WispView {
                icon: icon_data_uri(&self.dir, &w.id),
                description: description(&w.id).to_string(),
                id: w.id,
                name: w.name,
                color: w.color,
                active: w.active,
                expression: w.expression,
            })
            .collect())
    }

    /// The switched-on wisps, for the settings tab.
    pub fn settings_wisps(&self) -> Result<Vec<SettingsWisp>> {
        let conn = self.conn()?;
        Ok(store::active_wisps(&conn)?
            .into_iter()
            .map(|w| SettingsWisp {
                icon: icon_data_uri(&self.dir, &w.id),
                intervals: intervals(&w.id),
                last_fetched: w.last_fetched_at.as_deref().map(utc_to_local_text).unwrap_or_default(),
                fetch_interval: w.fetch_interval_seconds,
                id: w.id,
                name: w.name,
                color: w.color,
            })
            .collect())
    }

    /// Gives a wisp a new name (trimmed, cleaned and cut to `MAX_NAME_CHARS`). `false` if nothing is left.
    pub fn rename_wisp(&self, wisp_id: &str, new_name: &str) -> Result<bool> {
        let name = safe::clean_text(new_name, MAX_NAME_CHARS);
        if name.is_empty() {
            return Ok(false);
        }
        let conn = self.conn()?;
        if store::get_wisp(&conn, wisp_id)?.is_none() {
            return Ok(false);
        }
        store::update_wisp(&conn, wisp_id, &store::WispChanges { name: Some(name), ..Default::default() })?;
        Ok(true)
    }

    /// Changes the wisp's colour and redraws its icons. `false` for a colour that isn't `#rrggbb`.
    pub fn recolor_wisp(&self, wisp_id: &str, color: &str) -> Result<bool> {
        let conn = self.conn()?;
        Ok(appearance::set_appearance(&conn, &self.dir, wisp_id, Some(color), None).is_ok())
    }

    /// Changes the wisp's face and redraws its icons. `false` for a face that doesn't exist.
    pub fn set_expression(&self, wisp_id: &str, expression: &str) -> Result<bool> {
        let conn = self.conn()?;
        Ok(appearance::set_appearance(&conn, &self.dir, wisp_id, None, Some(expression)).is_ok())
    }

    pub fn set_fetch_interval(&self, wisp_id: &str, seconds: i64) -> Result<()> {
        // 0 means "only at start-up"; anything else under a minute would hammer the sources.
        let seconds = if seconds <= 0 { 0 } else { seconds.max(60) };
        store::set_fetch_interval(&self.conn()?, wisp_id, seconds)
    }

    /// Switches a wisp on or off. Gives the new state, or `None` if there is no such wisp.
    pub fn toggle_active(&self, wisp_id: &str) -> Result<Option<bool>> {
        let conn = self.conn()?;
        let Some(wisp) = store::get_wisp(&conn, wisp_id)? else { return Ok(None) };
        store::set_wisp_active(&conn, wisp_id, !wisp.active)?;
        Ok(Some(!wisp.active))
    }

    // ---- items ----

    pub fn channels(&self, wisp_id: &str) -> Result<Vec<ChannelView>> {
        let conn = self.conn()?;
        let mut channels = Vec::new();
        for channel in store::wisp_channels(&conn, wisp_id)? {
            let items = store::items(&conn, &channel.id, Some(ITEMS_PER_CHANNEL))?
                .into_iter()
                .map(|i| ItemView {
                    id: i.id,
                    text: i.text,
                    read: i.read,
                    url: i.url.unwrap_or_default(),
                    price: i.price,
                    discount: i.discount_percent,
                })
                .collect();
            channels.push(ChannelView {
                unread: store::unread_count(&conn, &channel.id)?,
                id: channel.id,
                title: channel.title,
                color: channel.color,
                items,
            });
        }
        Ok(channels)
    }

    pub fn activity_feed(&self) -> Result<Vec<FeedItem>> {
        let conn = self.conn()?;
        let mut icons_by_wisp = std::collections::HashMap::new();
        let mut feed = Vec::new();
        for entry in store::recent_items(&conn, FEED_LENGTH)? {
            let icon = icons_by_wisp
                .entry(entry.wisp_id.clone())
                .or_insert_with(|| icon_data_uri(&self.dir, &entry.wisp_id))
                .clone();
            feed.push(FeedItem {
                item_id: entry.item_id,
                text: entry.text,
                read: entry.read,
                url: entry.url.unwrap_or_default(),
                wisp_name: entry.wisp_name,
                wisp_icon: icon,
                channel_id: entry.channel_id,
            });
        }
        Ok(feed)
    }

    pub fn mark_item_read(&self, item_id: i64) -> Result<()> {
        store::mark_read(&self.conn()?, item_id)
    }

    pub fn mark_all_read(&self, channel_id: &str) -> Result<()> {
        store::mark_all_read(&self.conn()?, channel_id)
    }

    pub fn dismiss_item(&self, item_id: i64) -> Result<()> {
        store::dismiss_item(&self.conn()?, item_id)
    }

    /// Removes the channel's read items from view.
    pub fn clear_read(&self, channel_id: &str) -> Result<()> {
        store::dismiss_read(&self.conn()?, channel_id)
    }

    /// Marks an item read and gives back the link to open, if it has a safe one. The link comes from
    /// the database, never from the page, and only `https` links are given out.
    pub fn open_item(&self, item_id: i64) -> Result<Option<String>> {
        let conn = self.conn()?;
        store::mark_read(&conn, item_id)?;
        Ok(store::item_url(&conn, item_id)?.filter(|url| url.starts_with("https://")))
    }

    // ---- checking ----

    /// Runs one wisp's check right now (on this thread, which may take a while for the internet
    /// wisps). `None` if there is no such wisp.
    pub fn fetch_now(&self, wisp_id: &str) -> Result<Option<FetchReport>> {
        match runtime::fetcher_for(wisp_id) {
            Some(mut fetcher) => self.fetch_with(fetcher.as_mut()).map(Some),
            None => Ok(None),
        }
    }

    /// Runs a check with the given fetcher and says how it went. A failure or crash inside the
    /// check is logged and reported as `ok: false`, never passed on.
    pub fn fetch_with(&self, fetcher: &mut dyn Fetcher) -> Result<FetchReport> {
        let conn = self.conn()?;
        let id = fetcher.wisp_id().to_string();
        let before = store::wisp_unread(&conn, &id)?;
        let result = catch_unwind(AssertUnwindSafe(|| fetcher.fetch(&conn)))
            .unwrap_or_else(|_| Err("the check crashed".to_string()));
        let ok = match result {
            Ok(()) => {
                store::mark_fetched(&conn, &id)?;
                true
            }
            Err(message) => {
                log::warn!("[{id}] check failed: {message}");
                false
            }
        };
        let after = store::wisp_unread(&conn, &id)?;
        Ok(FetchReport { wisp_id: id, ok, new_items: (after - before).max(0) })
    }

    // ---- Companion ----

    pub fn companion_notes(&self) -> Result<NotesView> {
        let conn = self.conn()?;
        let reminders = companion::list_reminders(&conn, self.now()).map_err(note_to_core)?;
        let ideas = companion::list_ideas(&conn).map_err(note_to_core)?;
        Ok(NotesView {
            reminders,
            ideas,
            empty_reminders: voice::EMPTY_REMINDERS,
            empty_ideas: voice::EMPTY_IDEAS,
        })
    }

    pub fn companion_mood(&self) -> Result<Feeling> {
        let conn = self.conn()?;
        let name = account::get_name(&self.dir);
        companion::describe_mood(&conn, self.now(), name.as_deref()).map_err(note_to_core)
    }

    pub fn companion_add_reminder(&self, text: &str, date: &str, time: &str, repeat: &str) -> ChangeResult {
        self.change(Some(voice::SAVED_REMINDER), |conn, now| {
            companion::create_reminder(conn, text, Some(date), Some(time), Some(repeat), now)
        })
    }

    pub fn companion_add_idea(&self, text: &str) -> ChangeResult {
        self.change(Some(voice::SAVED_IDEA), |conn, now| companion::create_idea(conn, text, now))
    }

    pub fn companion_update(&self, note_id: i64, text: &str, date: &str, time: &str, repeat: &str) -> ChangeResult {
        self.change(None, |conn, now| {
            companion::update_note(conn, note_id, text, Some(date), Some(time), Some(repeat), now)
        })
    }

    pub fn companion_complete(&self, note_id: i64) -> ChangeResult {
        self.change(None, |conn, now| companion::complete_reminder(conn, note_id, now))
    }

    pub fn companion_snooze(&self, note_id: i64, option: &str) -> ChangeResult {
        self.change(None, |conn, now| companion::snooze_reminder(conn, note_id, option, now))
    }

    pub fn companion_delete(&self, note_id: i64) -> ChangeResult {
        self.change(None, |conn, _| companion::delete_note(conn, note_id))
    }

    /// Runs a change to Companion's notes and answers in the shape the page reads. Problems the
    /// person can fix come back as pip's friendly sentence; anything else is logged and gets a
    /// general apology.
    fn change(
        &self,
        message: Option<&str>,
        action: impl FnOnce(&Connection, NaiveDateTime) -> std::result::Result<(), NoteError>,
    ) -> ChangeResult {
        let outcome = self.conn().map_err(NoteError::Core).and_then(|conn| action(&conn, self.now()));
        match outcome {
            Ok(()) => ChangeResult { ok: true, error: None, message: message.map(str::to_string) },
            Err(NoteError::Problem(problem)) => {
                ChangeResult { ok: false, error: Some(NoteError::Problem(problem).to_string()), message: None }
            }
            Err(other) => {
                log::warn!("could not change a note: {other}");
                ChangeResult { ok: false, error: Some(TROUBLE.to_string()), message: None }
            }
        }
    }
}

/// Companion's list calls report `NoteError`; for the page's lists only real trouble is possible.
fn note_to_core(error: NoteError) -> crate::Error {
    match error {
        NoteError::Core(e) => e,
        NoteError::Sql(e) => e.into(),
        NoteError::Problem(p) => crate::Error::Invalid(NoteError::Problem(p).to_string()),
    }
}

/// What a wisp says it is (shown on the Discover tab).
fn description(wisp_id: &str) -> &'static str {
    match wisp_id {
        id if id == deals::DEF.id => deals::DEF.description,
        id if id == github::DEF.id => github::DEF.description,
        id if id == companion::DEF.id => companion::DEF.description,
        _ => "",
    }
}

/// The check intervals offered for a wisp.
pub fn intervals(wisp_id: &str) -> Vec<i64> {
    if wisp_id == companion::DEF.id {
        companion::FETCH_INTERVALS.to_vec()
    } else {
        DEFAULT_INTERVALS.to_vec()
    }
}

/// SQLite's clock is UTC; the person sees their own time zone. Text that can't be read is shown as it is.
pub fn utc_to_local_text(utc_text: &str) -> String {
    match NaiveDateTime::parse_from_str(utc_text.trim(), "%Y-%m-%d %H:%M:%S") {
        Ok(naive) => chrono::Utc
            .from_utc_datetime(&naive)
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        Err(_) => utc_text.to_string(),
    }
}

/// A wisp's icon as `data:image/png;base64,...` so the page can show it with no file permissions
/// (empty if the icon isn't there yet).
fn icon_data_uri(data_dir: &Path, wisp_id: &str) -> String {
    let Ok(folder) = icons::folder(data_dir, wisp_id) else { return String::new() };
    match fs::read(folder.join("icon.png")) {
        Ok(bytes) => format!("data:image/png;base64,{}", base64(&bytes)),
        Err(_) => String::new(),
    }
}

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}
