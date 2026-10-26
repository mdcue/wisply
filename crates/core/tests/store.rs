//! Tests for reading and writing wisps, channels and items.

use rusqlite::Connection;
use wisply_core::schema;
use wisply_core::store::{self, NewItem, WispChanges};

/// A database with two wisps and three channels.
fn setup() -> Connection {
    let conn = schema::open_in_memory().unwrap();
    store::add_wisp(&conn, "deals", "Game Deals", "#ff8800").unwrap();
    store::add_wisp(&conn, "github", "GitHub", "#222222").unwrap();
    store::upsert_channel(&conn, "deals_hot", "deals", "Hot deals", "Big discounts", "#f00", 1).unwrap();
    store::upsert_channel(&conn, "deals_new", "deals", "New", "Fresh", "#0f0", 0).unwrap();
    store::upsert_channel(&conn, "gh_trending", "github", "Trending", "Popular", "#00f", 0).unwrap();
    conn
}

fn item(channel: &str, key: &str, text: &str) -> NewItem {
    NewItem { channel_id: channel.into(), dedupe_key: key.into(), text: text.into(), ..Default::default() }
}

#[test]
fn adding_a_wisp_twice_keeps_the_first_and_places_it_last() {
    let conn = setup();
    assert!(!store::add_wisp(&conn, "deals", "Other name", "#000").unwrap());
    let wisp = store::get_wisp(&conn, "deals").unwrap().unwrap();
    assert_eq!(wisp.name, "Game Deals");
    assert!(!wisp.active);
    assert_eq!(wisp.expression, "smiling");
    assert_eq!(wisp.fetch_interval_seconds, 3600);
    let ids: Vec<_> = store::all_wisps(&conn).unwrap().into_iter().map(|w| w.id).collect();
    assert_eq!(ids, ["deals", "github"]);
    assert_eq!(store::get_wisp(&conn, "nobody").unwrap(), None);
}

#[test]
fn active_wisps_only_lists_switched_on_ones() {
    let conn = setup();
    assert!(store::active_wisps(&conn).unwrap().is_empty());
    store::set_wisp_active(&conn, "github", true).unwrap();
    let active = store::active_wisps(&conn).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "github");
    store::set_wisp_active(&conn, "github", false).unwrap();
    assert!(store::active_wisps(&conn).unwrap().is_empty());
}

#[test]
fn update_wisp_changes_only_what_is_given() {
    let conn = setup();
    let changes = WispChanges { color: Some("#123456".into()), ..Default::default() };
    store::update_wisp(&conn, "deals", &changes).unwrap();
    let wisp = store::get_wisp(&conn, "deals").unwrap().unwrap();
    assert_eq!((wisp.name.as_str(), wisp.color.as_str()), ("Game Deals", "#123456"));
    store::update_wisp(&conn, "deals", &WispChanges::default()).unwrap();
    assert_eq!(store::get_wisp(&conn, "deals").unwrap().unwrap().color, "#123456");
}

#[test]
fn interval_and_fetch_time() {
    let conn = setup();
    store::set_fetch_interval(&conn, "deals", 900).unwrap();
    assert_eq!(store::get_wisp(&conn, "deals").unwrap().unwrap().fetch_interval_seconds, 900);
    assert_eq!(store::get_wisp(&conn, "deals").unwrap().unwrap().last_fetched_at, None);
    store::mark_fetched(&conn, "deals").unwrap();
    assert!(store::get_wisp(&conn, "deals").unwrap().unwrap().last_fetched_at.is_some());
}

#[test]
fn channels_come_back_in_their_set_order_and_update_in_place() {
    let conn = setup();
    let titles: Vec<_> = store::wisp_channels(&conn, "deals").unwrap().into_iter().map(|c| c.title).collect();
    assert_eq!(titles, ["New", "Hot deals"]);
    // Same id again: title changes, colour is kept.
    store::upsert_channel(&conn, "deals_hot", "deals", "Hotter deals", "x", "#999", 5).unwrap();
    let channels = store::wisp_channels(&conn, "deals").unwrap();
    assert_eq!(channels[1].title, "Hotter deals");
    assert_eq!(channels[1].color, "#f00");
    assert_eq!(channels.len(), 2);
}

#[test]
fn a_channel_needs_an_existing_wisp() {
    let conn = setup();
    assert!(store::upsert_channel(&conn, "c", "nobody", "t", "d", "#000", 0).is_err());
}

#[test]
fn the_same_item_is_only_added_once_even_in_another_channel() {
    let conn = setup();
    assert!(store::add_item(&conn, &item("deals_hot", "k1", "Game A")).unwrap());
    assert!(store::has_item(&conn, "k1").unwrap());
    assert!(!store::has_item(&conn, "k2").unwrap());
    assert!(!store::add_item(&conn, &item("deals_hot", "k1", "Game A again")).unwrap());
    assert!(!store::add_item(&conn, &item("deals_new", "k1", "Game A elsewhere")).unwrap());
    assert_eq!(store::items(&conn, "deals_hot", None).unwrap().len(), 1);
}

#[test]
fn items_are_newest_first_with_a_limit_and_keep_their_numbers() {
    let conn = setup();
    for n in 1..=5 {
        let mut it = item("deals_hot", &format!("k{n}"), &format!("Game {n}"));
        it.price = Some(n as f64);
        it.discount_percent = Some(n * 10);
        store::add_item(&conn, &it).unwrap();
    }
    let all = store::items(&conn, "deals_hot", None).unwrap();
    assert_eq!(all.len(), 5);
    assert_eq!(all[0].text, "Game 5");
    assert_eq!(all[0].price, Some(5.0));
    assert_eq!(all[0].discount_percent, Some(50));
    assert_eq!(all[0].stars, None);
    let two = store::items(&conn, "deals_hot", Some(2)).unwrap();
    assert_eq!(two.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["Game 5", "Game 4"]);
}

#[test]
fn github_items_keep_stars_and_url() {
    let conn = setup();
    let mut it = item("gh_trending", "g1", "owner/repo");
    it.stars = Some(1234);
    it.url = Some("https://github.com/owner/repo".into());
    store::add_item(&conn, &it).unwrap();
    let got = &store::items(&conn, "gh_trending", None).unwrap()[0];
    assert_eq!(got.stars, Some(1234));
    assert_eq!(got.discount_percent, None);
    assert_eq!(got.url.as_deref(), Some("https://github.com/owner/repo"));
}

#[test]
fn unread_counts_and_marking_read() {
    let conn = setup();
    store::add_item(&conn, &item("deals_hot", "a", "A")).unwrap();
    store::add_item(&conn, &item("deals_hot", "b", "B")).unwrap();
    store::add_item(&conn, &item("deals_new", "c", "C")).unwrap();
    store::add_item(&conn, &item("gh_trending", "d", "D")).unwrap();

    assert_eq!(store::unread_count(&conn, "deals_hot").unwrap(), 2);
    assert_eq!(store::wisp_unread(&conn, "deals").unwrap(), 3);
    assert_eq!(store::wisp_unread(&conn, "github").unwrap(), 1);
    let counts = store::unread_counts(&conn).unwrap();
    assert_eq!(counts.get("deals_hot"), Some(&2));
    assert_eq!(counts.get("gh_trending"), Some(&1));

    let first = store::items(&conn, "deals_hot", None).unwrap()[1].id;
    store::mark_read(&conn, first).unwrap();
    assert_eq!(store::unread_count(&conn, "deals_hot").unwrap(), 1);
    store::mark_all_read(&conn, "deals_hot").unwrap();
    assert_eq!(store::unread_count(&conn, "deals_hot").unwrap(), 0);
    assert!(!store::unread_counts(&conn).unwrap().contains_key("deals_hot"));
    assert!(store::items(&conn, "deals_hot", None).unwrap().iter().all(|i| i.read));
}

#[test]
fn dismissed_items_vanish_but_are_never_added_again() {
    let conn = setup();
    store::add_item(&conn, &item("deals_hot", "a", "A")).unwrap();
    store::add_item(&conn, &item("deals_hot", "b", "B")).unwrap();
    let newest = store::items(&conn, "deals_hot", None).unwrap()[0].id;
    store::dismiss_item(&conn, newest).unwrap();
    assert_eq!(store::items(&conn, "deals_hot", None).unwrap().len(), 1);
    assert_eq!(store::unread_count(&conn, "deals_hot").unwrap(), 1);
    // Found again later: still refused.
    assert!(!store::add_item(&conn, &item("deals_hot", "b", "B")).unwrap());
    assert_eq!(store::items(&conn, "deals_hot", None).unwrap().len(), 1);

    store::mark_all_read(&conn, "deals_hot").unwrap();
    store::dismiss_read(&conn, "deals_hot").unwrap();
    assert!(store::items(&conn, "deals_hot", None).unwrap().is_empty());
}

#[test]
fn recent_items_mix_active_wisps_only() {
    let conn = setup();
    store::add_item(&conn, &item("deals_hot", "a", "A")).unwrap();
    store::add_item(&conn, &item("gh_trending", "b", "B")).unwrap();
    store::add_item(&conn, &item("deals_new", "c", "C")).unwrap();
    assert!(store::recent_items(&conn, 30).unwrap().is_empty());

    store::set_wisp_active(&conn, "deals", true).unwrap();
    store::set_wisp_active(&conn, "github", true).unwrap();
    let recent = store::recent_items(&conn, 30).unwrap();
    assert_eq!(recent.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["C", "B", "A"]);
    assert_eq!(recent[1].wisp_id, "github");
    assert_eq!(recent[1].wisp_name, "GitHub");
    assert_eq!(recent[1].channel_id, "gh_trending");

    store::set_wisp_active(&conn, "github", false).unwrap();
    assert_eq!(store::recent_items(&conn, 30).unwrap().len(), 2);
    assert_eq!(store::recent_items(&conn, 1).unwrap().len(), 1);
}

#[test]
fn awkward_text_is_stored_as_text_and_never_run() {
    let conn = setup();
    let nasty = "x'); DROP TABLE items; --";
    store::add_item(&conn, &item("deals_hot", nasty, nasty)).unwrap();
    assert!(store::has_item(&conn, nasty).unwrap());
    assert_eq!(store::items(&conn, "deals_hot", None).unwrap()[0].text, nasty);
    assert!(store::add_wisp(&conn, nasty, nasty, "#000").unwrap());
    assert_eq!(store::get_wisp(&conn, nasty).unwrap().unwrap().name, nasty);
}

#[test]
fn deleting_a_wisp_removes_its_channels_and_items() {
    let conn = setup();
    store::add_item(&conn, &item("deals_hot", "a", "A")).unwrap();
    conn.execute("DELETE FROM wisps WHERE id = 'deals'", []).unwrap();
    assert!(store::wisp_channels(&conn, "deals").unwrap().is_empty());
    assert!(!store::has_item(&conn, "a").unwrap());
}