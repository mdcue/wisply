//! Decides when each wisp should check its source, and runs the checks.
//!
//! Same rules as the Python scheduler:
//! * every `TICK` the scheduler looks at every wisp that is switched on;
//! * a wisp is due once its check interval has passed (a few seconds early counts, so a
//!   one-minute interval is never missed by a hair);
//! * an interval of 0 means "only when the app starts";
//! * a failed check never stops the others, and that wisp waits 5 minutes before trying again.
//!
//! The scheduler doesn't know how to fetch anything. Each wisp provides a `Fetcher`, which keeps
//! this file free of the internet, so it can be tested completely.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::{schema, store, Result};

pub const TICK: Duration = Duration::from_secs(60);
pub const RETRY_AFTER_FAILURE_SECONDS: i64 = 300;
pub const DUE_TOLERANCE_SECONDS: i64 = 5;
/// Wisps checked more often than this don't write a log line for every check.
pub const QUIET_BELOW_SECONDS: i64 = 300;

/// What a wisp provides: its id, and a way to check its source and store what it finds.
pub trait Fetcher: Send {
    fn wisp_id(&self) -> &str;
    /// Check the source and store new items (with `store::add_item`). An `Err` message is logged
    /// and puts the wisp on a short cooldown.
    fn fetch(&mut self, conn: &Connection) -> std::result::Result<(), String>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Fetched,
    Failed(String),
}

/// One check that happened during a tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetch {
    pub wisp_id: String,
    pub outcome: Outcome,
}

pub struct Scheduler {
    fetchers: Vec<Box<dyn Fetcher>>,
    /// Wisp id -> the time (unix seconds) before which it must not be tried again.
    retry_not_before: HashMap<String, i64>,
}

/// The current time as unix seconds.
pub fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Scheduler {
    pub fn new(fetchers: Vec<Box<dyn Fetcher>>) -> Scheduler {
        Scheduler { fetchers, retry_not_before: HashMap::new() }
    }

    /// One look at every wisp. Runs the checks that are due and says what happened.
    pub fn tick(&mut self, conn: &Connection, now: i64) -> Result<Vec<Fetch>> {
        let mut done = Vec::new();
        for index in 0..self.fetchers.len() {
            let id = self.fetchers[index].wisp_id().to_string();
            let Some(wisp) = store::get_wisp(conn, &id)? else { continue };
            if !wisp.active {
                continue;
            }
            if now < self.retry_not_before.get(&id).copied().unwrap_or(0) {
                continue;
            }
            if !is_due(conn, &wisp, now)? {
                continue;
            }
            if wisp.fetch_interval_seconds >= QUIET_BELOW_SECONDS {
                log::info!("Fetching {id}...");
            }
            done.push(self.run_one(index, conn, now)?);
        }
        Ok(done)
    }

    /// Called once when the app starts, for wisps set to "only on startup" (interval 0).
    pub fn fetch_startup_only(&mut self, conn: &Connection, now: i64) -> Result<Vec<Fetch>> {
        let mut done = Vec::new();
        for index in 0..self.fetchers.len() {
            let id = self.fetchers[index].wisp_id().to_string();
            let Some(wisp) = store::get_wisp(conn, &id)? else { continue };
            if wisp.active && wisp.fetch_interval_seconds == 0 {
                log::info!("Startup-only fetch for {id}...");
                done.push(self.run_one(index, conn, now)?);
            }
        }
        Ok(done)
    }

    /// Runs one wisp's check. A failure (or a crash inside it) is logged and put on a cooldown
    /// instead of stopping the scheduler or hitting the source again on every tick.
    fn run_one(&mut self, index: usize, conn: &Connection, now: i64) -> Result<Fetch> {
        let fetcher = &mut self.fetchers[index];
        let id = fetcher.wisp_id().to_string();
        let result = catch_unwind(AssertUnwindSafe(|| fetcher.fetch(conn)))
            .unwrap_or_else(|_| Err("the check crashed".to_string()));
        match result {
            Ok(()) => {
                self.retry_not_before.remove(&id);
                store::mark_fetched(conn, &id)?;
                Ok(Fetch { wisp_id: id, outcome: Outcome::Fetched })
            }
            Err(message) => {
                log::warn!(
                    "[{id}] fetch failed, will retry in {} minutes: {message}",
                    RETRY_AFTER_FAILURE_SECONDS / 60
                );
                self.retry_not_before.insert(id.clone(), now + RETRY_AFTER_FAILURE_SECONDS);
                Ok(Fetch { wisp_id: id, outcome: Outcome::Failed(message) })
            }
        }
    }
}

/// True once a wisp's check interval has passed. "Only on startup" (0) wisps are never due here.
fn is_due(conn: &Connection, wisp: &store::Wisp, now: i64) -> Result<bool> {
    if wisp.fetch_interval_seconds == 0 {
        return Ok(false);
    }
    let Some(last) = &wisp.last_fetched_at else { return Ok(true) };
    // SQLite turns its own UTC text into seconds; text it can't understand comes back empty,
    // and then the wisp counts as due (it will be re-stamped properly after the next check).
    let then: Option<i64> =
        conn.query_row("SELECT CAST(strftime('%s', ?1) AS INTEGER)", [last], |row| row.get(0))?;
    Ok(match then {
        Some(then) => now - then >= wisp.fetch_interval_seconds - DUE_TOLERANCE_SECONDS,
        None => true,
    })
}

/// A running scheduler thread.
pub struct Handle {
    stop: Sender<()>,
    thread: JoinHandle<()>,
}

impl Handle {
    /// Asks the thread to stop and waits for it to finish.
    pub fn stop(self) {
        let _ = self.stop.send(());
        let _ = self.thread.join();
    }
}

/// Starts the scheduler on its own thread. It opens its own connection to the database at
/// `db_path`, does the "only on startup" checks once, then looks at the wisps every `tick`.
/// `on_complete` is called after each look (for example to refresh the tray badge).
pub fn spawn(
    db_path: PathBuf,
    fetchers: Vec<Box<dyn Fetcher>>,
    tick: Duration,
    on_complete: impl Fn() + Send + 'static,
) -> Handle {
    let (stop, wake) = mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let conn = match schema::open(&db_path) {
            Ok(conn) => conn,
            Err(error) => {
                log::error!("scheduler could not open the database: {error}");
                return;
            }
        };
        let mut scheduler = Scheduler::new(fetchers);
        if let Err(error) = scheduler.fetch_startup_only(&conn, unix_now()) {
            log::error!("startup checks failed: {error}");
        }
        on_complete();
        // Waiting on the channel is the sleep: it ends early the moment `stop` is sent.
        while let Err(RecvTimeoutError::Timeout) = wake.recv_timeout(tick) {
            if let Err(error) = scheduler.tick(&conn, unix_now()) {
                log::error!("scheduler tick failed: {error}");
            }
            on_complete();
        }
    });
    Handle { stop, thread }
}