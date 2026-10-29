//! Tests for what the running app uses: setting up the wisps, the hover text, and the scheduler
//! started the way the app starts it.

use std::sync::mpsc;
use std::time::Duration;

use chrono::NaiveDate;
use rusqlite::Connection;
use wisply_core::runtime::{self, FACES};
use wisply_core::scheduler::{Fetcher, Handle};
use wisply_core::store::{self, NewItem};
use wisply_core::wisps::companion;
use wisply_core::{paths, schema};

fn setup() -> Connection {
    let conn = schema::open_in_memory().unwrap();
    runtime::register_wisps(&conn).unwrap();
    conn
}

fn item(channel: &str, key: &str) -> NewItem {
    NewItem { channel_id: channel.into(), dedupe_key: key.into(), text: key.into(), ..Default::default() }
}

#[test]
fn registering_creates_the_three_wisps_switched_off() {
    let conn = setup();
    let wisps = store::all_wisps(&conn).unwrap();
    let shown: Vec<_> = wisps.iter().map(|w| (w.id.as_str(), w.name.as_str(), w.active)).collect();
    assert_eq!(
        shown,
        [("game_deals", "ware-Weaver", false), ("github", "Gitty", false), ("companion", "pip", false)]
    );
    let channels = |id: &str| store::wisp_channels(&conn, id).unwrap().len();
    assert_eq!((channels("game_deals"), channels("github"), channels("companion")), (2, 6, 2));
}

#[test]
fn registering_again_keeps_what_the_user_changed() {
    let conn = setup();
    store::set_wisp_active(&conn, "github", true).unwrap();
    store::set_fetch_interval(&conn, "github", 7200).unwrap();
    store::update_wisp(&conn, "github", &store::WispChanges { color: Some("#123456".into()), ..Default::default() }).unwrap();
    runtime::register_wisps(&conn).unwrap();
    runtime::register_wisps(&conn).unwrap();
    let github = store::get_wisp(&conn, "github").unwrap().unwrap();
    assert!(github.active);
    assert_eq!((github.fetch_interval_seconds, github.color.as_str()), (7200, "#123456"));
    assert_eq!(store::all_wisps(&conn).unwrap().len(), 3);
}

#[test]
fn preparing_a_new_folder_creates_the_database_and_the_wisps_once() {
    let dir = tempfile::tempdir().unwrap();
    assert!(runtime::prepare(dir.path()).unwrap().is_none());
    assert!(runtime::prepare(dir.path()).unwrap().is_none());
    let conn = schema::open(&dir.path().join(paths::DB_FILE)).unwrap();
    assert_eq!(store::all_wisps(&conn).unwrap().len(), 3);
}

#[test]
fn there_is_one_fetcher_per_wisp_in_a_steady_order() {
    let ids: Vec<String> = runtime::fetchers().iter().map(|f| f.wisp_id().to_string()).collect();
    assert_eq!(ids, ["game_deals", "github", "companion"]);
}

#[test]
fn the_hover_text_counts_unread_items_of_switched_on_wisps_only() {
    let conn = setup();
    assert_eq!(runtime::tooltip(&conn).unwrap(), "Wisply");

    store::add_item(&conn, &item("deals", "d1")).unwrap();
    store::add_item(&conn, &item("gh_day", "g1")).unwrap();
    store::add_item(&conn, &item("gh_day", "g2")).unwrap();
    store::add_item(&conn, &item("co_today", "c1")).unwrap();
    assert_eq!(runtime::tooltip(&conn).unwrap(), "Wisply", "everything is switched off");

    store::set_wisp_active(&conn, "github", true).unwrap();
    assert_eq!(runtime::tooltip(&conn).unwrap(), "Wisply: 2 unread");
    store::set_wisp_active(&conn, "companion", true).unwrap();
    assert_eq!(runtime::tooltip(&conn).unwrap(), "Wisply: 3 unread");

    store::mark_all_read(&conn, "gh_day").unwrap();
    assert_eq!(runtime::tooltip(&conn).unwrap(), "Wisply: 1 unread");
}

#[test]
fn the_face_list_gives_pip_a_face_for_every_mood() {
    let faces: Vec<String> = FACES.iter().map(|f| f.to_string()).collect();
    let conn = setup();
    let at = |h| NaiveDate::from_ymd_opt(2026, 10, 26).unwrap().and_hms_opt(h, 0, 0).unwrap();
    let face = |conn: &Connection| store::get_wisp(conn, "companion").unwrap().unwrap().expression;

    companion::check(&conn, at(10), &faces).unwrap();
    assert_eq!(face(&conn), "happy");
    companion::check(&conn, at(23), &faces).unwrap();
    assert_eq!(face(&conn), "blank"); // no "sleepy" picture exists, so the last choice is used
    companion::add_reminder(&conn, "Due", at(9), companion::Repeat::None, at(8)).unwrap();
    companion::check(&conn, at(10), &faces).unwrap();
    assert_eq!(face(&conn), "nervous"); // no "worried" picture either
}

#[test]
fn the_face_list_matches_the_pictures_in_the_repository_when_they_are_there() {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/assets/expressions");
    let Ok(entries) = std::fs::read_dir(&folder) else { return };
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.path().file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect();
    names.sort();
    let mut listed: Vec<String> = FACES.iter().map(|f| f.to_string()).collect();
    listed.sort();
    assert_eq!(names, listed);
}

/// Waits for an update that says `wanted`, for up to five seconds.
fn wait_for(updates: &mpsc::Receiver<String>, wanted: &str) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match updates.recv_timeout(left) {
            Ok(text) if text == wanted => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

/// A stand-in for a wisp: each check adds one new item to Companion's Today channel.
struct Counting(u32);

impl Fetcher for Counting {
    fn wisp_id(&self) -> &str {
        "companion"
    }
    fn fetch(&mut self, conn: &Connection) -> Result<(), String> {
        self.0 += 1;
        store::add_item(conn, &item("co_today", &format!("n{}", self.0))).map(|_| ()).map_err(|e| e.to_string())
    }
}

#[test]
fn the_scheduler_started_like_the_app_runs_checks_and_reports_the_hover_text() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = schema::open(&dir.path().join(paths::DB_FILE)).unwrap();
        runtime::register_wisps(&conn).unwrap();
        store::set_wisp_active(&conn, "companion", true).unwrap();
        store::set_fetch_interval(&conn, "companion", 0).unwrap(); // only at start: runs once, at once
    }
    let (send, updates) = mpsc::channel();
    let handle = runtime::start_with(dir.path(), vec![Box::new(Counting(0))], Duration::from_millis(30), move |update| {
        let _ = send.send(update.tooltip);
    });
    assert!(wait_for(&updates, "Wisply: 1 unread"), "the start-up check should have run and been reported");
    handle.stop();
}

#[test]
fn the_handle_can_be_kept_in_the_app_s_shared_state() {
    fn needs_send_and_sync<T: Send + Sync + 'static>() {}
    needs_send_and_sync::<Handle>();
}

/// A stand-in that finds nothing.
struct Quiet;

impl Fetcher for Quiet {
    fn wisp_id(&self) -> &str {
        "companion"
    }
    fn fetch(&mut self, _conn: &Connection) -> Result<(), String> {
        Ok(())
    }
}

fn first_update(fetcher: Box<dyn Fetcher>, before_start: impl Fn(&Connection)) -> runtime::Update {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = schema::open(&dir.path().join(paths::DB_FILE)).unwrap();
        runtime::register_wisps(&conn).unwrap();
        store::set_wisp_active(&conn, "companion", true).unwrap();
        store::set_fetch_interval(&conn, "companion", 0).unwrap();
        before_start(&conn);
    }
    let (send, updates) = mpsc::channel();
    let handle = runtime::start_with(dir.path(), vec![fetcher], Duration::from_millis(30), move |update| {
        let _ = send.send(update);
    });
    let update = updates.recv_timeout(Duration::from_secs(5)).expect("an update should arrive");
    handle.stop();
    update
}

#[test]
fn an_update_carries_the_badge_and_who_found_something_new() {
    let update = first_update(Box::new(Counting(0)), |_| {});
    assert_eq!(
        update,
        runtime::Update { tooltip: "Wisply: 1 unread".into(), has_unread: true, new_from: vec![("pip".into(), 1)] }
    );
}

#[test]
fn items_that_were_already_there_at_start_are_not_new() {
    let update = first_update(Box::new(Quiet), |conn| {
        store::add_item(conn, &item("co_today", "old")).unwrap();
    });
    assert!(update.has_unread, "the badge still shows the old unread item");
    assert!(update.new_from.is_empty(), "but nobody gets a pop-up for it");
}

#[test]
fn nothing_unread_means_a_plain_icon() {
    let conn = setup();
    let now = runtime::status(&conn).unwrap();
    assert_eq!(now, runtime::Update { tooltip: "Wisply".into(), has_unread: false, new_from: vec![] });
}
