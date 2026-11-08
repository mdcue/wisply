//! Tests for the scheduler. Time is passed in as a number, so these never wait.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use wisply_core::scheduler::{self, Fetch, Fetcher, Outcome, Scheduler};
use wisply_core::{schema, store};

/// 2026-10-26 12:00:00 UTC, as unix seconds. The tests set a wisp's last check to exactly this.
const T0: i64 = 1_793_016_000;
const T0_TEXT: &str = "2026-10-26 12:00:00";

#[derive(Clone, Copy)]
enum Behaviour {
    Works,
    Fails,
    Panics,
}

struct Fake {
    id: String,
    calls: Arc<AtomicUsize>,
    behaviour: Behaviour,
}

impl Fetcher for Fake {
    fn wisp_id(&self) -> &str {
        &self.id
    }
    fn fetch(&mut self, _conn: &Connection) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.behaviour {
            Behaviour::Works => Ok(()),
            Behaviour::Fails => Err("no internet".to_string()),
            Behaviour::Panics => panic!("boom"),
        }
    }
}

fn fake(id: &str, behaviour: Behaviour) -> (Box<dyn Fetcher>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (Box::new(Fake { id: id.into(), calls: calls.clone(), behaviour }), calls)
}

/// One active wisp "w" with the given interval, last checked at T0.
fn setup(interval: i64, last: Option<&str>) -> Connection {
    let conn = schema::open_in_memory().unwrap();
    store::add_wisp(&conn, "w", "W", "#000").unwrap();
    store::set_wisp_active(&conn, "w", true).unwrap();
    store::set_fetch_interval(&conn, "w", interval).unwrap();
    conn.execute("UPDATE wisps SET last_fetched_at = ?1 WHERE id = 'w'", [last]).unwrap();
    conn
}

fn count(calls: &Arc<AtomicUsize>) -> usize {
    calls.load(Ordering::SeqCst)
}

#[test]
fn a_wisp_never_checked_is_due() {
    let conn = setup(3600, None);
    let (f, calls) = fake("w", Behaviour::Works);
    let mut s = Scheduler::new(vec![f]);
    let done = s.tick(&conn, T0).unwrap();
    assert_eq!(done, [Fetch { wisp_id: "w".into(), outcome: Outcome::Fetched }]);
    assert_eq!(count(&calls), 1);
}

#[test]
fn a_wisp_is_due_only_after_its_interval() {
    let conn = setup(3600, Some(T0_TEXT));
    let (f, calls) = fake("w", Behaviour::Works);
    let mut s = Scheduler::new(vec![f]);
    s.tick(&conn, T0 + 3000).unwrap();
    assert_eq!(count(&calls), 0);
    s.tick(&conn, T0 + 3594).unwrap(); // 6 seconds early: not yet
    assert_eq!(count(&calls), 0);
    s.tick(&conn, T0 + 3595).unwrap(); // 5 seconds early counts as due
    assert_eq!(count(&calls), 1);
}

#[test]
fn a_wisp_switched_off_is_skipped() {
    let conn = setup(3600, None);
    store::set_wisp_active(&conn, "w", false).unwrap();
    let (f, calls) = fake("w", Behaviour::Works);
    Scheduler::new(vec![f]).tick(&conn, T0).unwrap();
    assert_eq!(count(&calls), 0);
}

#[test]
fn a_fetcher_for_an_unknown_wisp_is_skipped() {
    let conn = setup(3600, None);
    let (f, calls) = fake("nobody", Behaviour::Works);
    assert!(Scheduler::new(vec![f]).tick(&conn, T0).unwrap().is_empty());
    assert_eq!(count(&calls), 0);
}

#[test]
fn startup_only_wisps_run_once_at_start_and_never_in_a_tick() {
    let conn = setup(0, None);
    let (f, calls) = fake("w", Behaviour::Works);
    let mut s = Scheduler::new(vec![f]);
    assert!(s.tick(&conn, T0).unwrap().is_empty());
    assert_eq!(count(&calls), 0);
    let done = s.fetch_startup_only(&conn, T0).unwrap();
    assert_eq!(done.len(), 1);
    assert_eq!(count(&calls), 1);
    assert!(s.tick(&conn, T0 + 100_000).unwrap().is_empty());
    assert_eq!(count(&calls), 1);
}

#[test]
fn startup_checks_leave_ordinary_wisps_alone() {
    let conn = setup(3600, None);
    let (f, calls) = fake("w", Behaviour::Works);
    assert!(Scheduler::new(vec![f]).fetch_startup_only(&conn, T0).unwrap().is_empty());
    assert_eq!(count(&calls), 0);
}

#[test]
fn a_successful_check_is_stamped_so_it_is_not_due_again_at_once() {
    let conn = setup(3600, None);
    let (f, calls) = fake("w", Behaviour::Works);
    let mut s = Scheduler::new(vec![f]);
    s.tick(&conn, scheduler::unix_now()).unwrap();
    assert!(store::get_wisp(&conn, "w").unwrap().unwrap().last_fetched_at.is_some());
    s.tick(&conn, scheduler::unix_now() + 60).unwrap();
    assert_eq!(count(&calls), 1);
}

#[test]
fn a_failed_check_waits_five_minutes_and_is_not_stamped() {
    let conn = setup(900, Some(T0_TEXT));
    let (f, calls) = fake("w", Behaviour::Fails);
    let mut s = Scheduler::new(vec![f]);
    let now = T0 + 900;
    let done = s.tick(&conn, now).unwrap();
    assert_eq!(done[0].outcome, Outcome::Failed("no internet".into()));
    assert_eq!(count(&calls), 1);
    assert_eq!(store::get_wisp(&conn, "w").unwrap().unwrap().last_fetched_at.as_deref(), Some(T0_TEXT));

    s.tick(&conn, now + 60).unwrap();
    s.tick(&conn, now + 299).unwrap();
    assert_eq!(count(&calls), 1);
    s.tick(&conn, now + 300).unwrap();
    assert_eq!(count(&calls), 2);
}

#[test]
fn a_crash_inside_a_check_is_caught() {
    let conn = setup(3600, None);
    let (f, calls) = fake("w", Behaviour::Panics);
    let mut s = Scheduler::new(vec![f]);
    let done = s.tick(&conn, T0).unwrap();
    assert_eq!(done[0].outcome, Outcome::Failed("the check crashed".into()));
    s.tick(&conn, T0 + 10).unwrap();
    assert_eq!(count(&calls), 1);
}

#[test]
fn one_failing_wisp_does_not_stop_the_others() {
    let conn = setup(3600, None);
    store::add_wisp(&conn, "v", "V", "#000").unwrap();
    store::set_wisp_active(&conn, "v", true).unwrap();
    let (bad, bad_calls) = fake("w", Behaviour::Fails);
    let (good, good_calls) = fake("v", Behaviour::Works);
    let done = Scheduler::new(vec![bad, good]).tick(&conn, T0).unwrap();
    assert_eq!(done.len(), 2);
    assert_eq!((count(&bad_calls), count(&good_calls)), (1, 1));
}

#[test]
fn a_garbled_last_check_time_counts_as_due() {
    let conn = setup(3600, Some("yesterday-ish"));
    let (f, calls) = fake("w", Behaviour::Works);
    Scheduler::new(vec![f]).tick(&conn, T0).unwrap();
    assert_eq!(count(&calls), 1);
}

#[test]
fn the_thread_runs_startup_checks_then_ticks_and_stops_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wisply.db");
    {
        let conn = schema::open(&db).unwrap();
        store::add_wisp(&conn, "w", "W", "#000").unwrap();
        store::set_wisp_active(&conn, "w", true).unwrap();
        store::set_fetch_interval(&conn, "w", 1).unwrap();
        store::add_wisp(&conn, "once", "Once", "#000").unwrap();
        store::set_wisp_active(&conn, "once", true).unwrap();
        store::set_fetch_interval(&conn, "once", 0).unwrap();
    }
    let (timed, timed_calls) = fake("w", Behaviour::Works);
    let (once, once_calls) = fake("once", Behaviour::Works);
    let looks = Arc::new(AtomicUsize::new(0));
    let looks_seen = looks.clone();
    let handle = scheduler::spawn(db, vec![timed, once], Duration::from_millis(30), move || {
        looks_seen.fetch_add(1, Ordering::SeqCst);
    });

    // The interval is 1 second, so the timed wisp is first due about a second after it was stamped.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while count(&timed_calls) == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    handle.stop();

    assert_eq!(count(&once_calls), 1);
    assert!(count(&timed_calls) >= 1);
    assert!(looks.load(Ordering::SeqCst) >= 2);
}
/// Always fails, and tells the scheduler how long the source asked it to wait.
struct Asks {
    calls: Arc<AtomicUsize>,
    wait: Option<i64>,
}

impl Fetcher for Asks {
    fn wisp_id(&self) -> &str {
        "w"
    }
    fn fetch(&mut self, _conn: &Connection) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err("rate limited".to_string())
    }
    fn retry_wait_seconds(&self) -> Option<i64> {
        self.wait
    }
}

fn asks(wait: Option<i64>) -> (Box<dyn Fetcher>, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (Box::new(Asks { calls: calls.clone(), wait }), calls)
}

/// Fails on the checks marked `false` in the list, works on `true` (and on every check after the list).
struct Pattern {
    calls: Arc<AtomicUsize>,
    results: Vec<bool>,
}

impl Fetcher for Pattern {
    fn wisp_id(&self) -> &str {
        "w"
    }
    fn fetch(&mut self, _conn: &Connection) -> Result<(), String> {
        let number = self.calls.fetch_add(1, Ordering::SeqCst);
        if self.results.get(number).copied().unwrap_or(true) {
            Ok(())
        } else {
            Err("no internet".to_string())
        }
    }
}

#[test]
fn the_wait_after_failures_grows_and_stops_at_an_hour() {
    let waits: Vec<i64> = (0..=8).map(scheduler::backoff_seconds).collect();
    assert_eq!(waits, [300, 300, 600, 1200, 2400, 3600, 3600, 3600, 3600]);
}

#[test]
fn a_wisp_that_keeps_failing_waits_longer_each_time() {
    let conn = setup(900, Some(T0_TEXT));
    let (f, calls) = fake("w", Behaviour::Fails);
    let mut s = Scheduler::new(vec![f]);
    let t1 = T0 + 900;
    s.tick(&conn, t1).unwrap(); // first failure: 5 minutes
    assert_eq!(count(&calls), 1);
    let t2 = t1 + 300;
    s.tick(&conn, t2 - 1).unwrap();
    assert_eq!(count(&calls), 1);
    s.tick(&conn, t2).unwrap(); // second failure: 10 minutes
    assert_eq!(count(&calls), 2);
    s.tick(&conn, t2 + 599).unwrap();
    assert_eq!(count(&calls), 2);
    let t3 = t2 + 600;
    s.tick(&conn, t3).unwrap(); // third failure: 20 minutes
    assert_eq!(count(&calls), 3);
    s.tick(&conn, t3 + 1199).unwrap();
    assert_eq!(count(&calls), 3);
    s.tick(&conn, t3 + 1200).unwrap();
    assert_eq!(count(&calls), 4);
}

#[test]
fn one_success_starts_the_waits_over_from_five_minutes() {
    let conn = setup(900, None);
    let calls = Arc::new(AtomicUsize::new(0));
    let pattern = Pattern { calls: calls.clone(), results: vec![false, false, true, false] };
    let mut s = Scheduler::new(vec![Box::new(pattern)]);
    let t0 = scheduler::unix_now();
    s.tick(&conn, t0).unwrap(); // fails: waits 5 minutes
    s.tick(&conn, t0 + 300).unwrap(); // fails: waits 10 minutes
    s.tick(&conn, t0 + 900).unwrap(); // works
    assert_eq!(count(&calls), 3);
    let t4 = t0 + 900 + 1000;
    s.tick(&conn, t4).unwrap(); // fails again, but as a first failure: only 5 minutes
    assert_eq!(count(&calls), 4);
    s.tick(&conn, t4 + 299).unwrap();
    assert_eq!(count(&calls), 4);
    s.tick(&conn, t4 + 300).unwrap();
    assert_eq!(count(&calls), 5);
}

#[test]
fn a_source_that_asks_for_longer_is_given_longer() {
    let conn = setup(900, None);
    let (f, calls) = asks(Some(3000));
    let mut s = Scheduler::new(vec![f]);
    s.tick(&conn, T0).unwrap();
    assert_eq!(count(&calls), 1);
    s.tick(&conn, T0 + 2999).unwrap();
    assert_eq!(count(&calls), 1);
    s.tick(&conn, T0 + 3000).unwrap();
    assert_eq!(count(&calls), 2);
}

#[test]
fn a_source_asking_for_less_than_our_own_wait_does_not_shorten_it() {
    let conn = setup(900, None);
    let (f, calls) = asks(Some(10));
    let mut s = Scheduler::new(vec![f]);
    s.tick(&conn, T0).unwrap();
    s.tick(&conn, T0 + 299).unwrap();
    assert_eq!(count(&calls), 1);
    s.tick(&conn, T0 + 300).unwrap();
    assert_eq!(count(&calls), 2);
}

#[test]
fn a_silly_long_ask_is_cut_to_two_hours() {
    let conn = setup(900, None);
    let (f, calls) = asks(Some(10 * 86_400));
    let mut s = Scheduler::new(vec![f]);
    s.tick(&conn, T0).unwrap();
    s.tick(&conn, T0 + scheduler::MAX_SERVER_WAIT_SECONDS - 1).unwrap();
    assert_eq!(count(&calls), 1);
    s.tick(&conn, T0 + scheduler::MAX_SERVER_WAIT_SECONDS).unwrap();
    assert_eq!(count(&calls), 2);
}

#[test]
fn a_negative_ask_counts_as_no_ask() {
    let conn = setup(900, None);
    let (f, calls) = asks(Some(-50));
    let mut s = Scheduler::new(vec![f]);
    s.tick(&conn, T0).unwrap();
    s.tick(&conn, T0 + 300).unwrap();
    assert_eq!(count(&calls), 2);
}