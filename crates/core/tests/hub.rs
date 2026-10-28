//! Tests for everything the Hub page asks for (the Rust version of the Python `bridge.py`).

use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use rusqlite::Connection;
use wisply_core::hub::{self, Hub};
use wisply_core::scheduler::Fetcher;
use wisply_core::store::{self, NewItem};
use wisply_core::wisps::companion_voice::{self as voice, Problem};
use wisply_core::{appearance, paths, runtime, schema};

fn now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 28).unwrap().and_hms_opt(12, 0, 0).unwrap()
}

/// A data folder with the database and the three wisps, and a Hub whose clock stands still.
fn setup() -> (tempfile::TempDir, Hub) {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = schema::open(&dir.path().join(paths::DB_FILE)).unwrap();
        runtime::register_wisps(&conn).unwrap();
    }
    let hub = Hub::new(dir.path()).with_clock(now);
    (dir, hub)
}

fn conn(dir: &tempfile::TempDir) -> Connection {
    schema::open(&dir.path().join(paths::DB_FILE)).unwrap()
}

fn item(channel: &str, key: &str, text: &str) -> NewItem {
    NewItem { channel_id: channel.into(), dedupe_key: key.into(), text: text.into(), ..Default::default() }
}

fn json<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

// --- small helpers ---

#[test]
fn base64_matches_the_standard_test_vectors() {
    for (plain, coded) in
        [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")]
    {
        assert_eq!(hub::base64(plain.as_bytes()), coded);
    }
    assert_eq!(hub::base64(&[0xff, 0xfe, 0xfd]), "//79");
}

#[test]
fn utc_times_are_shown_in_the_local_zone_and_odd_text_is_left_alone() {
    let shown = hub::utc_to_local_text("2026-10-28 07:30:00");
    let local = NaiveDateTime::parse_from_str(&shown, "%Y-%m-%d %H:%M:%S").unwrap();
    let back = Local.from_local_datetime(&local).earliest().unwrap().naive_utc();
    assert_eq!(back.to_string(), "2026-10-28 07:30:00", "same moment, written in local time");
    assert_eq!(hub::utc_to_local_text("yesterday-ish"), "yesterday-ish");
    assert_eq!(hub::utc_to_local_text(""), "");
}

#[test]
fn intervals_are_per_wisp() {
    assert_eq!(hub::intervals("companion"), [60, 300, 900, 1800, 3600]);
    assert_eq!(hub::intervals("github"), [0, 900, 1800, 3600, 7200, 14400]);
    assert_eq!(hub::intervals("anything_else"), hub::DEFAULT_INTERVALS);
}

// --- account ---

#[test]
fn an_account_can_be_made_once_there_is_a_name() {
    let (dir, hub) = setup();
    assert!(!hub.account_exists());
    assert!(!hub.create_account("   "));
    assert!(!hub.account_exists(), "an empty name writes nothing");
    assert!(hub.create_account("  Sam "));
    assert!(hub.account_exists());
    assert_eq!(wisply_core::account::get_name(dir.path()).as_deref(), Some("Sam"));
}

// --- wisps ---

#[test]
fn the_faces_on_offer_are_the_ten_pictures() {
    let (_dir, hub) = setup();
    let faces = hub.expressions();
    assert_eq!(faces.len(), 10);
    assert!(faces.contains(&"happy") && faces.contains(&"tongue"));
}

#[test]
fn wisps_come_with_description_and_icon_once_icons_exist() {
    let (dir, hub) = setup();
    let before = hub.wisps().unwrap();
    assert_eq!(before.iter().map(|w| w.id.as_str()).collect::<Vec<_>>(), ["game_deals", "github", "companion"]);
    assert!(before.iter().all(|w| !w.description.is_empty()), "every wisp says what it is");
    assert!(before.iter().all(|w| w.icon.is_empty()), "no icons drawn yet");
    assert!(before.iter().all(|w| !w.active));

    appearance::ensure_icons(&conn(&dir), dir.path()).unwrap();
    let after = hub.wisps().unwrap();
    assert!(after.iter().all(|w| w.icon.starts_with("data:image/png;base64,iVBOR")), "a PNG as a data address");
}

#[test]
fn wisps_turn_into_the_json_the_page_reads() {
    let (_dir, hub) = setup();
    let first = json(&hub.wisps().unwrap()[0]);
    let mut keys: Vec<_> = first.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["active", "color", "description", "expression", "icon", "id", "name"]);
}

#[test]
fn switching_a_wisp_on_and_off() {
    let (dir, hub) = setup();
    assert_eq!(hub.toggle_active("github").unwrap(), Some(true));
    assert!(store::get_wisp(&conn(&dir), "github").unwrap().unwrap().active);
    assert_eq!(hub.toggle_active("github").unwrap(), Some(false));
    assert_eq!(hub.toggle_active("nobody").unwrap(), None);
}

#[test]
fn settings_list_only_switched_on_wisps_with_their_intervals_and_last_check() {
    let (dir, hub) = setup();
    assert!(hub.settings_wisps().unwrap().is_empty());
    hub.toggle_active("companion").unwrap();
    hub.toggle_active("github").unwrap();
    store::mark_fetched(&conn(&dir), "github").unwrap();
    let wisps = hub.settings_wisps().unwrap();
    assert_eq!(wisps.len(), 2);
    let (github, companion) = (wisps.iter().find(|w| w.id == "github").unwrap(), wisps.iter().find(|w| w.id == "companion").unwrap());
    assert_eq!(companion.intervals, [60, 300, 900, 1800, 3600]);
    assert_eq!(companion.fetch_interval, 60);
    assert_eq!(companion.last_fetched, "", "never checked");
    assert_eq!(github.intervals, hub::DEFAULT_INTERVALS);
    assert_eq!(github.last_fetched.len(), 19, "a date and time in the page's format");
}

#[test]
fn renaming_trims_cleans_and_cuts() {
    let (dir, hub) = setup();
    assert!(hub.rename_wisp("github", "  Octo  ").unwrap());
    assert_eq!(store::get_wisp(&conn(&dir), "github").unwrap().unwrap().name, "Octo");
    assert!(!hub.rename_wisp("github", "   ").unwrap(), "nothing left, nothing changed");
    assert!(!hub.rename_wisp("nobody", "Ghost").unwrap());
    assert!(hub.rename_wisp("github", &"x".repeat(100)).unwrap());
    let name = store::get_wisp(&conn(&dir), "github").unwrap().unwrap().name;
    assert!(name.chars().count() <= hub::MAX_NAME_CHARS);
    assert!(hub.rename_wisp("github", "Line\u{202E}one\ntwo").unwrap());
    assert_eq!(store::get_wisp(&conn(&dir), "github").unwrap().unwrap().name, "Lineone two");
}

#[test]
fn recolouring_and_changing_the_face_redraw_the_icons_and_refuse_nonsense() {
    let (dir, hub) = setup();
    assert!(hub.recolor_wisp("github", "#123456").unwrap());
    assert!(hub.set_expression("github", "kiss").unwrap());
    let wisp = store::get_wisp(&conn(&dir), "github").unwrap().unwrap();
    assert_eq!((wisp.color.as_str(), wisp.expression.as_str()), ("#123456", "kiss"));
    assert!(dir.path().join("icons").join("github").join("icon.png").is_file());

    assert!(!hub.recolor_wisp("github", "blue").unwrap());
    assert!(!hub.set_expression("github", "grumpy").unwrap());
    assert!(!hub.recolor_wisp("nobody", "#ffffff").unwrap());
    let same = store::get_wisp(&conn(&dir), "github").unwrap().unwrap();
    assert_eq!((same.color.as_str(), same.expression.as_str()), ("#123456", "kiss"));
}

#[test]
fn intervals_are_kept_sensible() {
    let (dir, hub) = setup();
    for (given, kept) in [(900, 900), (0, 0), (-5, 0), (10, 60), (86_400, 86_400)] {
        hub.set_fetch_interval("github", given).unwrap();
        assert_eq!(store::get_wisp(&conn(&dir), "github").unwrap().unwrap().fetch_interval_seconds, kept, "{given}");
    }
}

// --- items ---

#[test]
fn channels_show_ten_newest_items_the_unread_count_and_the_page_s_field_names() {
    let (dir, hub) = setup();
    let c = conn(&dir);
    for n in 0..12 {
        store::add_item(&c, &item("co_today", &format!("k{n}"), &format!("item {n}"))).unwrap();
    }
    store::add_item(
        &c,
        &NewItem { price: Some(4.5), discount_percent: Some(80), url: Some("https://x.test/a".into()), ..item("deals", "d1", "deal") },
    )
    .unwrap();
    store::mark_read(&c, 1).unwrap();

    let companion = hub.channels("companion").unwrap();
    assert_eq!(companion.len(), 2);
    let today = &companion[0];
    assert_eq!(today.id, "co_today");
    assert_eq!(today.items.len(), 10, "only the ten newest");
    assert_eq!(today.items[0].text, "item 11");
    assert_eq!(today.unread, 11, "the count covers all items, not just the ten shown");
    assert_eq!(today.items[0].url, "", "no link means an empty one, like before");

    let deals = hub.channels("game_deals").unwrap();
    let hot = deals.iter().find(|c| c.id == "deals").unwrap();
    let shown = json(&hot.items[0]);
    assert_eq!(shown["price"], 4.5);
    assert_eq!(shown["discount"], 80);
    assert_eq!(shown["url"], "https://x.test/a");
    assert_eq!(shown["read"], false);
    assert!(hub.channels("nobody").unwrap().is_empty());
}

#[test]
fn the_activity_feed_mixes_switched_on_wisps_newest_first() {
    let (dir, hub) = setup();
    let c = conn(&dir);
    hub.toggle_active("companion").unwrap();
    hub.toggle_active("github").unwrap();
    store::add_item(&c, &item("co_today", "a", "from pip")).unwrap();
    store::add_item(&c, &item("gh_day", "b", "from gitty")).unwrap();
    store::add_item(&c, &item("deals", "c", "from a switched-off wisp")).unwrap();
    appearance::ensure_icons(&c, dir.path()).unwrap();
    let feed = hub.activity_feed().unwrap();
    let texts: Vec<_> = feed.iter().map(|f| f.text.as_str()).collect();
    assert_eq!(texts, ["from gitty", "from pip"]);
    assert_eq!(feed[0].wisp_name, "Gitty");
    assert!(feed[0].wisp_icon.starts_with("data:image/png;base64,"));
    assert_eq!(feed[0].channel_id, "gh_day");
}

#[test]
fn reading_and_clearing() {
    let (dir, hub) = setup();
    let c = conn(&dir);
    for n in 1..=4 {
        store::add_item(&c, &item("co_today", &format!("k{n}"), &format!("t{n}"))).unwrap();
    }
    hub.mark_item_read(1).unwrap();
    assert_eq!(store::unread_count(&c, "co_today").unwrap(), 3);
    hub.mark_all_read("co_today").unwrap();
    assert_eq!(store::unread_count(&c, "co_today").unwrap(), 0);
    hub.dismiss_item(2).unwrap();
    hub.clear_read("co_today").unwrap();
    assert!(hub.channels("companion").unwrap()[0].items.is_empty(), "read items were cleared from view");
}

#[test]
fn opening_an_item_reads_it_and_only_gives_out_safe_links_from_the_database() {
    let (dir, hub) = setup();
    let c = conn(&dir);
    store::add_item(&c, &NewItem { url: Some("https://github.com/a/b".into()), ..item("gh_day", "a", "A") }).unwrap();
    store::add_item(&c, &NewItem { url: Some("http://plain.test".into()), ..item("gh_day", "b", "B") }).unwrap();
    store::add_item(&c, &NewItem { url: Some("javascript:alert(1)".into()), ..item("gh_day", "c", "C") }).unwrap();
    store::add_item(&c, &item("gh_day", "d", "no link")).unwrap();
    assert_eq!(hub.open_item(1).unwrap().as_deref(), Some("https://github.com/a/b"));
    assert_eq!(hub.open_item(2).unwrap(), None);
    assert_eq!(hub.open_item(3).unwrap(), None);
    assert_eq!(hub.open_item(4).unwrap(), None);
    assert_eq!(hub.open_item(999).unwrap(), None);
    assert_eq!(store::unread_count(&c, "gh_day").unwrap(), 0, "opening marks read, even with no link");
}

// --- checking ---

struct Finds(u32);
impl Fetcher for Finds {
    fn wisp_id(&self) -> &str {
        "companion"
    }
    fn fetch(&mut self, conn: &Connection) -> Result<(), String> {
        for n in 0..self.0 {
            store::add_item(conn, &item("co_today", &format!("f{n}"), "found")).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

struct Breaks(bool);
impl Fetcher for Breaks {
    fn wisp_id(&self) -> &str {
        "github"
    }
    fn fetch(&mut self, _conn: &Connection) -> Result<(), String> {
        if self.0 {
            panic!("boom");
        }
        Err("no internet".to_string())
    }
}

#[test]
fn a_manual_check_reports_how_many_new_items_it_found() {
    let (dir, hub) = setup();
    let report = hub.fetch_with(&mut Finds(3)).unwrap();
    assert_eq!(report, hub::FetchReport { wisp_id: "companion".into(), ok: true, new_items: 3 });
    assert!(store::get_wisp(&conn(&dir), "companion").unwrap().unwrap().last_fetched_at.is_some(), "stamped");
    assert_eq!(hub.fetch_with(&mut Finds(3)).unwrap().new_items, 0, "the same items again are not new");
}

#[test]
fn a_failed_or_crashing_check_is_reported_not_passed_on_and_not_stamped() {
    let (dir, hub) = setup();
    for crash in [false, true] {
        let report = hub.fetch_with(&mut Breaks(crash)).unwrap();
        assert_eq!(report, hub::FetchReport { wisp_id: "github".into(), ok: false, new_items: 0 });
    }
    assert!(store::get_wisp(&conn(&dir), "github").unwrap().unwrap().last_fetched_at.is_none());
}

#[test]
fn checking_a_wisp_that_does_not_exist_gives_nothing() {
    let (_dir, hub) = setup();
    assert_eq!(hub.fetch_now("nobody").unwrap(), None);
}

#[test]
fn companion_can_be_checked_by_name_without_the_internet() {
    let (_dir, hub) = setup();
    let report = hub.fetch_now("companion").unwrap().expect("pip exists");
    assert!(report.ok);
}

// --- Companion ---

#[test]
fn saving_an_idea_or_reminder_says_so_in_pips_voice() {
    let (_dir, hub) = setup();
    let idea = hub.companion_add_idea("  paint the fence ");
    assert_eq!(idea, hub::ChangeResult { ok: true, error: None, message: Some(voice::SAVED_IDEA.into()) });
    let reminder = hub.companion_add_reminder("water plants", "2026-10-29", "09:00", "weekly");
    assert!(reminder.ok);
    assert_eq!(reminder.message.as_deref(), Some(voice::SAVED_REMINDER));
    let notes = hub.companion_notes().unwrap();
    assert_eq!(notes.ideas.len(), 1);
    assert_eq!(notes.ideas[0].text, "paint the fence");
    assert_eq!(notes.reminders.len(), 1);
    assert_eq!((notes.reminders[0].date.as_str(), notes.reminders[0].time.as_str(), notes.reminders[0].repeat.as_str()), ("2026-10-29", "09:00", "weekly"));
    assert_eq!(notes.empty_reminders, voice::EMPTY_REMINDERS);
    assert_eq!(notes.empty_ideas, voice::EMPTY_IDEAS);
}

#[test]
fn mistakes_come_back_as_a_friendly_sentence_and_nothing_is_saved() {
    let (_dir, hub) = setup();
    let empty = hub.companion_add_idea("   ");
    assert_eq!((empty.ok, empty.error), (false, Some(voice::error(Problem::Empty))));
    let past = hub.companion_add_reminder("too late", "2026-10-28", "11:00", "none");
    assert_eq!(past.error, Some(voice::error(Problem::Past)));
    let no_time = hub.companion_add_reminder("when?", "", "", "none");
    assert_eq!(no_time.error, Some(voice::error(Problem::When)));
    let notes = hub.companion_notes().unwrap();
    assert!(notes.ideas.is_empty() && notes.reminders.is_empty());
}

#[test]
fn the_page_gets_the_same_json_shape_as_before() {
    let (_dir, hub) = setup();
    let ok = json(&hub.companion_add_idea("x"));
    assert_eq!(ok, serde_json::json!({"ok": true, "message": voice::SAVED_IDEA}));
    let bad = json(&hub.companion_add_idea(""));
    assert_eq!(bad, serde_json::json!({"ok": false, "error": voice::error(Problem::Empty), "message": null}));
    let notes = json(&hub.companion_notes().unwrap());
    assert!(notes["reminders"].is_array() && notes["ideas"].is_array());
    let mood = json(&hub.companion_mood().unwrap());
    assert!(mood["mood"].is_string() && mood["greeting"].is_string() && mood["line"].is_string());
}

#[test]
fn editing_finishing_snoozing_and_deleting_notes() {
    let (_dir, hub) = setup();
    hub.companion_add_reminder("first", "2026-10-29", "09:00", "none");
    hub.companion_add_idea("an idea");
    let id = hub.companion_notes().unwrap().reminders[0].id;
    let idea_id = hub.companion_notes().unwrap().ideas[0].id;

    assert!(hub.companion_update(id, "first, edited", "2026-10-30", "10:30", "daily").ok);
    let edited = &hub.companion_notes().unwrap().reminders[0];
    assert_eq!((edited.text.as_str(), edited.date.as_str(), edited.time.as_str(), edited.repeat.as_str()), ("first, edited", "2026-10-30", "10:30", "daily"));

    assert!(hub.companion_snooze(id, "1h").ok);
    assert!(hub.companion_complete(id).ok);
    assert!(hub.companion_delete(idea_id).ok);
    assert!(hub.companion_notes().unwrap().ideas.is_empty());

    let gone = hub.companion_delete(999);
    assert!(gone.ok, "removing what is not there is fine, as before");
    assert_eq!(hub.companion_update(999, "x", "", "", "").error, Some(voice::error(Problem::NoteGone)));
    assert_eq!(hub.companion_snooze(999, "1h").error, Some(voice::error(Problem::ReminderGone)));
    assert_eq!(hub.companion_snooze(id, "forever").error, Some(voice::error(Problem::Snooze)));
}

#[test]
fn the_mood_greets_by_name_once_there_is_an_account() {
    let (_dir, hub) = setup();
    hub.create_account("Sam");
    let mood = hub.companion_mood().unwrap();
    assert!(mood.greeting.contains("Sam"), "{}", mood.greeting);
}

#[test]
fn trouble_that_is_not_the_users_fault_gets_a_general_apology() {
    let dir = tempfile::tempdir().unwrap(); // a folder where no database was ever made, and none can be opened
    let missing = dir.path().join("not-there");
    let hub = Hub::new(&missing).with_clock(now);
    let result = hub.companion_add_idea("hello");
    assert!(!result.ok);
    assert!(result.error.unwrap().starts_with("Something went wrong"));
}
