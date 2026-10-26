//! Tests for the Game Deals wisp, against a fake CheapShark running on this computer.

mod common;

use common::{ok, start, status, Route};
use rusqlite::Connection;
use wisply_core::http::{Client, MAX_BODY_BYTES};
use wisply_core::scheduler::{Fetcher, Outcome, Scheduler};
use wisply_core::wisps::deals::{self, DealsFetcher};
use wisply_core::{schema, store};

const STORES: &str = r#"[
    {"storeID": "1", "storeName": "Steam", "isActive": 1},
    {"storeID": "2", "storeName": "Dead Store", "isActive": 0},
    {"storeID": "7", "storeName": "GOG", "isActive": 1}
]"#;

const DEALS: &str = r#"[
    {"dealID": "AAA", "title": "Best Game", "salePrice": "5.99", "normalPrice": "19.99", "savings": "70.035", "storeID": "1"},
    {"dealID": "BBB", "title": "Second Game", "salePrice": "9.99", "normalPrice": "29.99", "savings": "66.7", "storeID": "7"},
    {"dealID": "CCC", "title": "Free One", "salePrice": "0.00", "normalPrice": "9.99", "savings": "100", "storeID": "1"},
    {"dealID": "DDD", "title": "Mystery", "salePrice": "3.00", "normalPrice": "6.00", "savings": "50", "storeID": "99"}
]"#;

const FREE: &str = r#"[
    {"dealID": "FFF", "title": "Gift Game", "salePrice": "0.00", "normalPrice": "14.99", "savings": "100", "storeID": "7"}
]"#;

fn routes() -> Vec<Route> {
    vec![
        ok("/stores", STORES),
        ok("/deals?upperPrice=60", DEALS),
        ok("/deals?upperPrice=0", FREE),
    ]
}

/// A database with the wisp registered and switched on, and a fetcher aimed at the fake site.
fn setup(routes: Vec<Route>) -> (Connection, DealsFetcher, common::Server) {
    let server = start(routes);
    let conn = schema::open_in_memory().unwrap();
    deals::register_wisp(&conn).unwrap();
    store::set_wisp_active(&conn, "game_deals", true).unwrap();
    let fetcher = DealsFetcher::with_client(Client::insecure_for_tests(), &server.base);
    (conn, fetcher, server)
}

fn texts(conn: &Connection, channel: &str) -> Vec<String> {
    store::items(conn, channel, None).unwrap().into_iter().map(|i| i.text).collect()
}

#[test]
fn registering_creates_the_wisp_and_its_channels_once() {
    let conn = schema::open_in_memory().unwrap();
    deals::register_wisp(&conn).unwrap();
    store::set_wisp_active(&conn, "game_deals", true).unwrap();
    store::update_wisp(
        &conn,
        "game_deals",
        &store::WispChanges { color: Some("#123456".into()), ..Default::default() },
    )
    .unwrap();
    deals::register_wisp(&conn).unwrap();

    let wisp = store::get_wisp(&conn, "game_deals").unwrap().unwrap();
    assert_eq!(wisp.name, "ware-Weaver");
    assert_eq!(wisp.color, "#123456"); // the user's change is kept
    assert!(wisp.active);
    let channels = store::wisp_channels(&conn, "game_deals").unwrap();
    assert_eq!(channels.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["free_games", "deals"]);
}

#[test]
fn a_fetch_stores_deals_and_free_games_in_the_right_channels() {
    let (conn, mut fetcher, server) = setup(routes());
    fetcher.fetch(&conn).unwrap();

    // The best deal is listed first; the free one (price 0) is not a "deal"; the unknown store is named.
    assert_eq!(
        texts(&conn, "deals"),
        [
            "Best Game - $5.99 (was $19.99, 70% off) (Steam)",
            "Second Game - $9.99 (was $29.99, 67% off) (GOG)",
            "Mystery - $3.00 (was $6.00, 50% off) (Unknown store)",
        ]
    );
    assert_eq!(texts(&conn, "free_games"), ["Gift Game is free right now! (normally $14.99) (GOG)"]);

    let first = &store::items(&conn, "deals", None).unwrap()[0];
    assert_eq!(first.price, Some(5.99));
    assert_eq!(first.discount_percent, Some(70));
    assert_eq!(first.stars, None);
    assert_eq!(first.url.as_deref(), Some("https://www.cheapshark.com/redirect?dealID=AAA"));
    let free = &store::items(&conn, "free_games", None).unwrap()[0];
    assert_eq!((free.price, free.discount_percent), (Some(0.0), Some(100)));

    // It said who it was, and asked for the right things.
    let seen = server.requests();
    assert_eq!(seen.len(), 3);
    assert!(seen.iter().all(|r| r.header("user-agent").is_some_and(|ua| ua.starts_with("wisply/"))));
    assert!(seen.iter().any(|r| r.target == "/deals?upperPrice=60"));
    assert!(seen.iter().any(|r| r.target == "/deals?upperPrice=0"));
}

#[test]
fn fetching_again_adds_nothing_new() {
    let (conn, mut fetcher, _server) = setup(routes());
    fetcher.fetch(&conn).unwrap();
    let before = store::items(&conn, "deals", None).unwrap().len();
    fetcher.fetch(&conn).unwrap();
    assert_eq!(store::items(&conn, "deals", None).unwrap().len(), before);
    assert_eq!(store::items(&conn, "free_games", None).unwrap().len(), 1);
}

#[test]
fn only_the_ten_best_deals_are_looked_at() {
    let many: Vec<String> = (1..=15)
        .map(|n| {
            format!(r#"{{"dealID":"D{n}","title":"Game {n}","salePrice":"1.00","normalPrice":"2.00","savings":"50","storeID":"1"}}"#)
        })
        .collect();
    let (conn, mut fetcher, _server) = setup(vec![
        ok("/stores", STORES),
        ok("/deals?upperPrice=60", &format!("[{}]", many.join(","))),
        ok("/deals?upperPrice=0", "[]"),
    ]);
    fetcher.fetch(&conn).unwrap();
    let items = texts(&conn, "deals");
    assert_eq!(items.len(), 10);
    assert!(items[0].starts_with("Game 1 -"));
    assert!(items[9].starts_with("Game 10 -"));
}

#[test]
fn savings_are_rounded_like_the_python_app_did() {
    let stores = serde_json::from_str(STORES).unwrap();
    let names = deals::store_names(&stores).unwrap();
    let conn = schema::open_in_memory().unwrap();
    deals::register_wisp(&conn).unwrap();
    let sample = serde_json::json!([
        {"dealID": "H1", "title": "A", "salePrice": "1", "normalPrice": "2", "savings": "68.5", "storeID": "1"},
        {"dealID": "H2", "title": "B", "salePrice": "1", "normalPrice": "2", "savings": "69.5", "storeID": "1"},
        {"dealID": "H3", "title": "C", "salePrice": "1", "normalPrice": "2", "storeID": "1"},
    ]);
    deals::save_deals(&conn, &sample, &names).unwrap();
    let by_text: Vec<_> =
        store::items(&conn, "deals", None).unwrap().into_iter().map(|i| (i.text, i.discount_percent)).collect();
    // Listed best first: A (68.5 -> 68, the even one), B (69.5 -> 70), C (no savings given -> 0).
    assert_eq!(by_text[0].1, Some(68));
    assert_eq!(by_text[1].1, Some(70));
    assert_eq!(by_text[2].1, Some(0));
}

#[test]
fn inactive_stores_are_left_out_of_the_names() {
    let names = deals::store_names(&serde_json::from_str(STORES).unwrap()).unwrap();
    assert_eq!(names.len(), 2);
    assert_eq!(names.get("1").map(String::as_str), Some("Steam"));
    assert!(!names.contains_key("2"));
}

#[test]
fn deals_missing_an_id_title_or_price_are_skipped_not_fatal() {
    let (conn, mut fetcher, _server) = setup(vec![
        ok("/stores", STORES),
        ok(
            "/deals?upperPrice=60",
            r#"[
                {"title": "No id", "salePrice": "1", "normalPrice": "2", "savings": "50", "storeID": "1"},
                {"dealID": "X1", "salePrice": "1", "normalPrice": "2", "savings": "50", "storeID": "1"},
                {"dealID": "X2", "title": "No price", "normalPrice": "2", "storeID": "1"},
                {"dealID": "X3", "title": "Bad price", "salePrice": "free!", "storeID": "1"},
                {"dealID": "X4", "title": "Fine", "salePrice": "1.50", "normalPrice": "3.00", "savings": "50", "storeID": "1"}
            ]"#,
        ),
        ok("/deals?upperPrice=0", "[]"),
    ]);
    fetcher.fetch(&conn).unwrap();
    assert_eq!(texts(&conn, "deals"), ["Fine - $1.50 (was $3.00, 50% off) (Steam)"]);
}

#[test]
fn real_style_deal_ids_are_used_in_the_link_exactly_as_they_arrive() {
    // CheapShark's ids come already percent-encoded. They must not be encoded again (%2B -> %252B
    // broke the links), and the duplicate-check key is the id as it arrived.
    let id = "6qGccabztKm%2BqifRAS%2FhBv6vy7xtBXxWwHWVn8qjpug%3D";
    let body = format!(
        r#"[{{"dealID": "{id}", "title": "Frosthaven", "salePrice": "25.19", "normalPrice": "39.99", "savings": "37", "storeID": "1"}}]"#
    );
    let (conn, mut fetcher, _server) =
        setup(vec![ok("/stores", STORES), ok("/deals?upperPrice=60", &body), ok("/deals?upperPrice=0", "[]")]);
    fetcher.fetch(&conn).unwrap();
    let item = &store::items(&conn, "deals", None).unwrap()[0];
    assert_eq!(
        item.url.as_deref(),
        Some("https://www.cheapshark.com/redirect?dealID=6qGccabztKm%2BqifRAS%2FhBv6vy7xtBXxWwHWVn8qjpug%3D")
    );
    assert!(store::has_item(&conn, id).unwrap());
}

#[test]
fn hostile_text_is_cleaned_and_an_odd_id_gets_no_link() {
    let (conn, mut fetcher, _server) = setup(vec![
        ok("/stores", STORES),
        ok(
            "/deals?upperPrice=60",
            r#"[
                {"dealID": "a b&c=d", "title": "Evil\nline two \u202E reversed", "salePrice": "1.00", "normalPrice": "2.00", "savings": "50", "storeID": "1"},
                {"dealID": "bad%zzescape", "title": "Odd", "salePrice": "1.00", "normalPrice": "2.00", "savings": "50", "storeID": "1"}
            ]"#,
        ),
        ok("/deals?upperPrice=0", "[]"),
    ]);
    fetcher.fetch(&conn).unwrap();
    let items = store::items(&conn, "deals", None).unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].text, "Evil line two reversed - $1.00 (was $2.00, 50% off) (Steam)");
    // An id that could add to the address, or has a broken escape, is kept but without a link.
    assert!(items.iter().all(|i| i.url.is_none()));
}

#[test]
fn a_very_long_title_is_capped() {
    let long = "W".repeat(5000);
    let body = format!(
        r#"[{{"dealID": "L1", "title": "{long}", "salePrice": "1.00", "normalPrice": "2.00", "savings": "50", "storeID": "1"}}]"#
    );
    let (conn, mut fetcher, _server) =
        setup(vec![ok("/stores", STORES), ok("/deals?upperPrice=60", &body), ok("/deals?upperPrice=0", "[]")]);
    fetcher.fetch(&conn).unwrap();
    let text = &texts(&conn, "deals")[0];
    assert!(text.chars().count() <= 300);
    assert!(text.contains("(Steam)"));
}

#[test]
fn a_server_error_is_reported_and_nothing_is_stored() {
    let (conn, mut fetcher, _server) = setup(vec![status("/stores", 500, "oops")]);
    let error = fetcher.fetch(&conn).unwrap_err();
    assert!(error.contains("500"), "got {error}");
    assert!(store::items(&conn, "deals", None).unwrap().is_empty());
}

#[test]
fn a_reply_that_is_not_json_or_not_a_list_is_an_error() {
    let (conn, mut fetcher, _server) = setup(vec![ok("/stores", "<html>sorry</html>")]);
    assert!(fetcher.fetch(&conn).unwrap_err().contains("isn't JSON"));

    let (conn, mut fetcher, _server) = setup(vec![ok("/stores", r#"{"error": "nope"}"#)]);
    assert!(fetcher.fetch(&conn).unwrap_err().contains("not a list"));
}

#[test]
fn a_giant_reply_is_refused() {
    let giant = "x".repeat(MAX_BODY_BYTES as usize + 1000);
    let (conn, mut fetcher, _server) = setup(vec![ok("/stores", &giant)]);
    let error = fetcher.fetch(&conn).unwrap_err();
    assert!(error.contains("could not read the reply"), "got {error}");
}

#[test]
fn redirects_are_not_followed() {
    let redirect = Route {
        target: "/stores",
        status: 302,
        body: String::new(),
        headers: vec![("Location", "http://127.0.0.1:9/elsewhere".to_string())],
    };
    let (conn, mut fetcher, server) = setup(vec![redirect]);
    assert!(fetcher.fetch(&conn).is_err());
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn the_real_client_only_talks_https_to_the_listed_site() {
    let fetcher = DealsFetcher::new();
    let conn = schema::open_in_memory().unwrap();
    // Nothing is sent: the client refuses these before connecting.
    let client = Client::strict(&["www.cheapshark.com"]);
    let plain = url::Url::parse("http://www.cheapshark.com/api/1.0/stores").unwrap();
    assert!(client.get(&plain, &[]).unwrap_err().contains("only https"));
    let other = url::Url::parse("https://evil.example/api/1.0/stores").unwrap();
    assert!(client.get(&other, &[]).unwrap_err().contains("not on the allowed list"));
    // (The fetcher itself is never run here: that would use the real internet.)
    assert_eq!(fetcher.wisp_id(), "game_deals");
    drop(conn);
}

#[test]
fn the_scheduler_runs_the_wisp_and_a_failure_puts_it_on_cooldown() {
    let (conn, fetcher, _server) = setup(routes());
    let mut scheduler = Scheduler::new(vec![Box::new(fetcher)]);
    let done = scheduler.tick(&conn, wisply_core::scheduler::unix_now()).unwrap();
    assert_eq!(done[0].outcome, Outcome::Fetched);
    assert!(store::get_wisp(&conn, "game_deals").unwrap().unwrap().last_fetched_at.is_some());
    assert!(!store::items(&conn, "deals", None).unwrap().is_empty());

    let (conn, fetcher, _server) = setup(vec![status("/stores", 503, "down")]);
    let mut scheduler = Scheduler::new(vec![Box::new(fetcher)]);
    let now = wisply_core::scheduler::unix_now();
    assert!(matches!(scheduler.tick(&conn, now).unwrap()[0].outcome, Outcome::Failed(_)));
    assert!(scheduler.tick(&conn, now + 60).unwrap().is_empty());
}