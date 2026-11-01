//! Companion ("pip"): reminders that come due, and a saved idea brought back each morning. It only
//! reads the user's own notes, so it needs no internet and checks every minute.
//!
//! Times here are the clock on the user's wall ("local time", no time zone), written as
//! `2026-10-26 14:30`, exactly as the Python app stored them. Every function that needs "now" is
//! given it, so tests can use any moment they like.

use chrono::{Duration, Local, Months, NaiveDateTime, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::companion_voice::{self as voice, Mood, Problem};
use super::{register, ChannelDef, WispDef};
use crate::safe::MAX_ITEM_TEXT;
use crate::scheduler::Fetcher;
use crate::store::{self, NewItem, WispChanges};

pub const TODAY: &str = "co_today";
pub const IDEAS: &str = "co_ideas";

pub const DEF: WispDef = WispDef {
    id: "companion",
    name: "pip",
    color: "#ff7a59",
    description: "Reminds you what's coming up and brings your saved ideas back.",
    channels: &[
        ChannelDef { id: TODAY, title: "Today", description: "Reminders that are due, and what is coming up", color: "#ff7a59" },
        ChannelDef { id: IDEAS, title: "Idea of the day", description: "One of your saved ideas, picked each morning", color: "#f5c542" },
    ],
    expression: "smiling",
    // Companion only reads your own notes, so it can check every minute.
    fetch_interval: Some(60),
};

/// The check intervals the Hub offers for Companion, in seconds.
pub const FETCH_INTERVALS: [i64; 5] = [60, 300, 900, 1800, 3600];

pub const TIME_FORMAT: &str = "%Y-%m-%d %H:%M";
/// A reminder fired later than this many minutes says when it was really due.
const LATE_MINUTES: i64 = 60;
/// The idea of the day appears from this hour on.
const IDEA_HOUR: u32 = 6;
/// Between `SLEEPY_FROM` and `SLEEPY_UNTIL` the face looks sleepy.
const SLEEPY_FROM: u32 = 23;
const SLEEPY_UNTIL: u32 = 5;
/// Longest note the Hub will accept (in characters).
pub const MAX_TEXT: usize = 300;
/// "Snooze until tomorrow" means tomorrow at this hour.
const TOMORROW_HOUR: u32 = 9;

pub fn register_wisp(conn: &Connection) -> crate::Result<()> {
    register(conn, &DEF)
}

/// Something went wrong with a note. `Problem` ones are the user's to fix and read as a friendly
/// sentence (`to_string()` gives it, in pip's voice); the others are database trouble.
#[derive(Debug, thiserror::Error)]
pub enum NoteError {
    #[error("{}", voice::error(*.0))]
    Problem(Problem),
    #[error(transparent)]
    Core(#[from] crate::Error),
    #[error("database problem: {0}")]
    Sql(#[from] rusqlite::Error),
}

impl From<Problem> for NoteError {
    fn from(problem: Problem) -> NoteError {
        NoteError::Problem(problem)
    }
}

type Result<T> = std::result::Result<T, NoteError>;

/// How often a reminder repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    None,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl Repeat {
    pub fn name(self) -> &'static str {
        match self {
            Repeat::None => "none",
            Repeat::Daily => "daily",
            Repeat::Weekly => "weekly",
            Repeat::Monthly => "monthly",
            Repeat::Yearly => "yearly",
        }
    }

    /// What the user picked. Nothing (or blank) means "none"; anything unknown is an error.
    fn parse(text: Option<&str>) -> Result<Repeat> {
        match text.unwrap_or("none").trim() {
            "" | "none" => Ok(Repeat::None),
            "daily" => Ok(Repeat::Daily),
            "weekly" => Ok(Repeat::Weekly),
            "monthly" => Ok(Repeat::Monthly),
            "yearly" => Ok(Repeat::Yearly),
            _ => Err(Problem::Repeat.into()),
        }
    }

    /// What the database holds. Text it doesn't know counts as "none", so one odd row can't stop
    /// everything else.
    fn from_db(text: &str) -> Repeat {
        Repeat::parse(Some(text)).unwrap_or(Repeat::None)
    }
}

// --- storing notes ---

fn stamp(moment: NaiveDateTime) -> String {
    moment.format(TIME_FORMAT).to_string()
}

fn read_stamp(text: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, TIME_FORMAT).ok()
}

pub fn add_reminder(conn: &Connection, text: &str, due: NaiveDateTime, repeat: Repeat, now: NaiveDateTime) -> Result<()> {
    conn.execute(
        "INSERT INTO companion_notes (kind, text, due_at, repeat_rule, created_at) VALUES ('reminder', ?1, ?2, ?3, ?4)",
        params![text.trim(), stamp(due), repeat.name(), stamp(now)],
    )?;
    Ok(())
}

pub fn add_idea(conn: &Connection, text: &str, now: NaiveDateTime) -> Result<()> {
    conn.execute(
        "INSERT INTO companion_notes (kind, text, created_at) VALUES ('idea', ?1, ?2)",
        params![text.trim(), stamp(now)],
    )?;
    Ok(())
}

pub fn delete_note(conn: &Connection, note_id: i64) -> Result<()> {
    conn.execute("DELETE FROM companion_notes WHERE id = ?1", [note_id])?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Note {
    pub id: i64,
    pub kind: String,
    pub text: String,
    pub due_at: Option<String>,
    pub repeat: String,
    pub done: bool,
}

/// Every saved note (a developer's view; the Hub uses `list_reminders` and `list_ideas`).
pub fn list_notes(conn: &Connection) -> Result<Vec<Note>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, text, due_at, repeat_rule, done FROM companion_notes ORDER BY kind, due_at, id",
    )?;
    let notes = stmt
        .query_map([], |row| {
            Ok(Note {
                id: row.get(0)?,
                kind: row.get(1)?,
                text: row.get(2)?,
                due_at: row.get(3)?,
                repeat: row.get(4)?,
                done: row.get::<_, i64>(5)? != 0,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(notes)
}

// --- dates ---

/// The first repeat of `due` that is still in the future, so a reminder that was missed for days
/// fires once and moves on instead of firing for every missed day. A month that is too short
/// pulls the day back (31 January + 1 month = 28 February), like the Python app did.
fn next_due(mut due: NaiveDateTime, rule: Repeat, now: NaiveDateTime) -> NaiveDateTime {
    while due <= now {
        let next = match rule {
            Repeat::Daily => due.checked_add_signed(Duration::days(1)),
            Repeat::Weekly => due.checked_add_signed(Duration::weeks(1)),
            Repeat::Monthly => due.checked_add_months(Months::new(1)),
            Repeat::Yearly => due.checked_add_months(Months::new(12)),
            Repeat::None => None,
        };
        match next {
            Some(next) => due = next,
            None => break,
        }
    }
    due
}

// --- notes, as the Hub shows and changes them ---

fn clean_text(text: &str) -> Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Problem::Empty.into());
    }
    if text.chars().count() > MAX_TEXT {
        return Err(Problem::TooLong { limit: MAX_TEXT }.into());
    }
    Ok(text.to_string())
}

fn parse_when(date_text: Option<&str>, time_text: Option<&str>) -> Result<NaiveDateTime> {
    let (Some(date), Some(time)) = (date_text, time_text) else { return Err(Problem::When.into()) };
    NaiveDateTime::parse_from_str(&format!("{date} {time}"), TIME_FORMAT).map_err(|_| Problem::When.into())
}

fn check_not_past(due: NaiveDateTime, now: NaiveDateTime) -> Result<()> {
    if due < now - Duration::minutes(1) {
        return Err(Problem::Past.into());
    }
    Ok(())
}

fn day_label(due: NaiveDateTime, now: NaiveDateTime) -> String {
    match (due.date() - now.date()).num_days() {
        0 => format!("Today {}", due.format("%H:%M")),
        1 => format!("Tomorrow {}", due.format("%H:%M")),
        -1 => format!("Yesterday {}", due.format("%H:%M")),
        _ => due.format("%a %d %b, %H:%M").to_string(),
    }
}

/// A reminder as the Hub shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reminder {
    pub id: i64,
    pub text: String,
    pub date: String,
    pub time: String,
    pub label: String,
    pub repeat: String,
    /// `"due"` once its time has come (it stays until marked done or snoozed), otherwise `"upcoming"`.
    pub status: &'static str,
}

/// Reminders that are not finished, soonest first.
pub fn list_reminders(conn: &Connection, now: NaiveDateTime) -> Result<Vec<Reminder>> {
    let mut stmt = conn.prepare(
        "SELECT id, text, due_at, repeat_rule FROM companion_notes \
         WHERE kind = 'reminder' AND done = 0 AND due_at IS NOT NULL ORDER BY due_at, id",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut reminders = Vec::new();
    for (id, text, due_at, rule) in rows {
        let Some(due) = read_stamp(&due_at) else {
            log::warn!("reminder {id} has a time I can't read ({due_at:?}); leaving it out");
            continue;
        };
        reminders.push(Reminder {
            id,
            text,
            date: due.format("%Y-%m-%d").to_string(),
            time: due.format("%H:%M").to_string(),
            label: day_label(due, now),
            repeat: Repeat::from_db(&rule).name().to_string(),
            status: if due <= now { "due" } else { "upcoming" },
        });
    }
    Ok(reminders)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Idea {
    pub id: i64,
    pub text: String,
}

/// Saved ideas, newest first.
pub fn list_ideas(conn: &Connection) -> Result<Vec<Idea>> {
    let mut stmt = conn.prepare("SELECT id, text FROM companion_notes WHERE kind = 'idea' ORDER BY id DESC")?;
    let ideas = stmt
        .query_map([], |row| Ok(Idea { id: row.get(0)?, text: row.get(1)? }))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ideas)
}

pub fn create_reminder(
    conn: &Connection,
    text: &str,
    date_text: Option<&str>,
    time_text: Option<&str>,
    repeat: Option<&str>,
    now: NaiveDateTime,
) -> Result<()> {
    let text = clean_text(text)?;
    let due = parse_when(date_text, time_text)?;
    check_not_past(due, now)?;
    add_reminder(conn, &text, due, Repeat::parse(repeat)?, now)
}

pub fn create_idea(conn: &Connection, text: &str, now: NaiveDateTime) -> Result<()> {
    add_idea(conn, &clean_text(text)?, now)
}

/// Changes a note. An idea only changes its text. A reminder also takes a new time and repeat, and
/// comes back (not done) if it had been finished. Keeping a time that has already passed is fine;
/// picking a new time in the past is not.
pub fn update_note(
    conn: &Connection,
    note_id: i64,
    text: &str,
    date_text: Option<&str>,
    time_text: Option<&str>,
    repeat: Option<&str>,
    now: NaiveDateTime,
) -> Result<()> {
    let row: Option<(String, Option<String>)> = conn
        .query_row("SELECT kind, due_at FROM companion_notes WHERE id = ?1", [note_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((kind, stored_due)) = row else { return Err(Problem::NoteGone.into()) };
    let text = clean_text(text)?;
    if kind == "idea" {
        conn.execute("UPDATE companion_notes SET text = ?1 WHERE id = ?2", params![text, note_id])?;
        return Ok(());
    }
    let due = parse_when(date_text, time_text)?;
    if Some(stamp(due)) != stored_due {
        check_not_past(due, now)?;
    }
    let repeat = Repeat::parse(repeat)?;
    conn.execute(
        "UPDATE companion_notes SET text = ?1, due_at = ?2, repeat_rule = ?3, done = 0 WHERE id = ?4",
        params![text, stamp(due), repeat.name(), note_id],
    )?;
    Ok(())
}

/// One-off reminders are finished. A repeating one moves on to its next time, so marking it done
/// skips this round.
pub fn complete_reminder(conn: &Connection, note_id: i64, now: NaiveDateTime) -> Result<()> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT due_at, repeat_rule FROM companion_notes WHERE id = ?1 AND kind = 'reminder' AND due_at IS NOT NULL",
            [note_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((due_at, rule)) = row else { return Ok(()) };
    let rule = Repeat::from_db(&rule);
    let due = read_stamp(&due_at);
    if rule == Repeat::None || due.is_none() {
        conn.execute("UPDATE companion_notes SET done = 1 WHERE id = ?1", [note_id])?;
        return Ok(());
    }
    let due = due.unwrap_or(now);
    let following = next_due(due, rule, now.max(due));
    conn.execute("UPDATE companion_notes SET due_at = ?1 WHERE id = ?2", params![stamp(following), note_id])?;
    Ok(())
}

fn snooze_time(option: &str, now: NaiveDateTime) -> Result<NaiveDateTime> {
    let later = match option {
        "10m" => now + Duration::minutes(10),
        "1h" => now + Duration::hours(1),
        "tomorrow" => (now + Duration::days(1)).with_hour(TOMORROW_HOUR).and_then(|t| t.with_minute(0)).unwrap_or(now),
        _ => return Err(Problem::Snooze.into()),
    };
    Ok(later.with_second(0).and_then(|t| t.with_nanosecond(0)).unwrap_or(later))
}

/// Moves a reminder to later (`"10m"`, `"1h"` or `"tomorrow"`). A repeating reminder keeps its own
/// schedule: the snooze is a one-off copy, and the original moves on as if it had been done.
pub fn snooze_reminder(conn: &Connection, note_id: i64, option: &str, now: NaiveDateTime) -> Result<()> {
    let later = snooze_time(option, now)?;
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT text, repeat_rule FROM companion_notes WHERE id = ?1 AND kind = 'reminder'",
            [note_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((text, rule)) = row else { return Err(Problem::ReminderGone.into()) };
    let tx = conn.unchecked_transaction()?;
    if Repeat::from_db(&rule) == Repeat::None {
        tx.execute("UPDATE companion_notes SET due_at = ?1, done = 0 WHERE id = ?2", params![stamp(later), note_id])?;
    } else {
        add_reminder(&tx, &text, later, Repeat::None, now)?;
        complete_reminder(&tx, note_id, now)?;
    }
    tx.commit()?;
    Ok(())
}

// --- making the items ---

fn add_today(conn: &Connection, key: String, text: String) -> Result<bool> {
    let item = NewItem { channel_id: TODAY.into(), dedupe_key: key, text, ..Default::default() };
    Ok(store::add_item(conn, &item)?)
}

fn fire_reminders(conn: &Connection, now: NaiveDateTime) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT id, text, due_at, repeat_rule FROM companion_notes \
         WHERE kind = 'reminder' AND done = 0 AND due_at IS NOT NULL",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(stmt);
    let tomorrow = (now + Duration::days(1)).date();
    for (id, text, due_at, rule) in rows {
        let Some(due) = read_stamp(&due_at) else {
            log::warn!("reminder {id} has a time I can't read ({due_at:?}); skipping it");
            continue;
        };
        let text = crate::safe::clean_text(&text, MAX_ITEM_TEXT);
        if due.date() == tomorrow {
            add_today(conn, format!("rem:{id}:{due_at}:eve"), voice::eve_item(&text, &due.format("%H:%M").to_string()))?;
        }
        if due > now {
            continue;
        }
        let late = now - due > Duration::minutes(LATE_MINUTES);
        let was_due = late.then(|| due.format("%a %d %b, %H:%M").to_string());
        add_today(conn, format!("rem:{id}:{due_at}"), voice::due_item(&text, was_due.as_deref()))?;
        let rule = Repeat::from_db(&rule);
        if rule != Repeat::None {
            conn.execute(
                "UPDATE companion_notes SET due_at = ?1 WHERE id = ?2",
                params![stamp(next_due(due, rule, now)), id],
            )?;
        }
    }
    Ok(())
}

/// Once a day, from `IDEA_HOUR`: one saved idea. Ideas never shown come first, then the one shown
/// longest ago (ties are picked at random).
fn surface_idea(conn: &Connection, now: NaiveDateTime) -> Result<()> {
    if now.hour() < IDEA_HOUR {
        return Ok(());
    }
    let key = format!("idea:{}", now.format("%Y-%m-%d"));
    if store::has_item(conn, &key)? {
        return Ok(());
    }
    let picked: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, text FROM companion_notes WHERE kind = 'idea' \
             ORDER BY last_shown IS NOT NULL, last_shown, RANDOM() LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((note_id, text)) = picked else { return Ok(()) };
    let item = NewItem {
        channel_id: IDEAS.into(),
        dedupe_key: key,
        text: voice::idea_item(&crate::safe::clean_text(&text, MAX_ITEM_TEXT)),
        ..Default::default()
    };
    if store::add_item(conn, &item)? {
        conn.execute("UPDATE companion_notes SET last_shown = ?1 WHERE id = ?2", params![stamp(now), note_id])?;
    }
    Ok(())
}

// --- the mood and the face ---

fn count_due(conn: &Connection, now: NaiveDateTime) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM companion_notes WHERE kind = 'reminder' AND done = 0 AND due_at <= ?1",
        [stamp(now)],
        |r| r.get(0),
    )?)
}

fn mood(conn: &Connection, now: NaiveDateTime) -> Result<Mood> {
    if now.hour() >= SLEEPY_FROM || now.hour() < SLEEPY_UNTIL {
        return Ok(Mood::Sleepy);
    }
    let today_unread = store::unread_count(conn, TODAY)?;
    if today_unread > 0 || count_due(conn, now)? > 0 {
        return Ok(Mood::Worried);
    }
    if today_unread + store::unread_count(conn, IDEAS)? == 0 {
        return Ok(Mood::Happy);
    }
    Ok(Mood::Calm)
}

/// Sets the wisp's face to match its mood, using the first face name from the mood's list that
/// exists in `available`. With no list, or no match, the face is left alone.
fn update_face(conn: &Connection, now: NaiveDateTime, available: &[String]) -> Result<()> {
    let Some(wisp) = store::get_wisp(conn, DEF.id)? else { return Ok(()) };
    let wanted = mood(conn, now)?.expressions().iter().find(|name| available.iter().any(|a| a == **name));
    if let Some(name) = wanted {
        if *name != wisp.expression {
            store::update_wisp(conn, DEF.id, &WispChanges { expression: Some(name.to_string()), ..Default::default() })?;
        }
    }
    Ok(())
}

/// How Companion feels right now, for the top of its panel in the Hub.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Feeling {
    pub mood: &'static str,
    pub greeting: String,
    pub line: String,
}

/// `name` is the user's name from the account file, if there is one.
pub fn describe_mood(conn: &Connection, now: NaiveDateTime, name: Option<&str>) -> Result<Feeling> {
    let mood = mood(conn, now)?;
    Ok(Feeling {
        mood: mood.name(),
        greeting: voice::greeting(now, mood, name),
        line: voice::mood_line(
            mood,
            count_due(conn, now)?,
            store::unread_count(conn, TODAY)?,
            store::unread_count(conn, IDEAS)?,
        ),
    })
}

/// One check: fire what is due, bring back the idea of the day, and set the face. All or nothing,
/// so a failure half way leaves nothing half done.
pub fn check(conn: &Connection, now: NaiveDateTime, available_faces: &[String]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    fire_reminders(&tx, now)?;
    surface_idea(&tx, now)?;
    update_face(&tx, now, available_faces)?;
    tx.commit()?;
    Ok(())
}

/// What the scheduler runs for Companion.
pub struct CompanionFetcher {
    /// `None` means "the real time"; tests set a fixed one.
    now: Option<NaiveDateTime>,
    /// The face names that exist (the app fills this in). Empty means "leave the face alone".
    faces: Vec<String>,
}

impl CompanionFetcher {
    pub fn new() -> CompanionFetcher {
        CompanionFetcher { now: None, faces: Vec::new() }
    }

    pub fn with_faces(mut self, names: Vec<String>) -> CompanionFetcher {
        self.faces = names;
        self
    }

    /// For tests: pretend it is this moment.
    pub fn with_now(mut self, now: NaiveDateTime) -> CompanionFetcher {
        self.now = Some(now);
        self
    }
}

impl Default for CompanionFetcher {
    fn default() -> Self {
        CompanionFetcher::new()
    }
}

impl Fetcher for CompanionFetcher {
    fn wisp_id(&self) -> &str {
        DEF.id
    }

    fn fetch(&mut self, conn: &Connection) -> std::result::Result<(), String> {
        let now = self.now.unwrap_or_else(|| Local::now().naive_local());
        check(conn, now, &self.faces).map_err(|e| e.to_string())
    }
}