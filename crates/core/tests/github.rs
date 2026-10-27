//! Tests for the GitHub wisp, against a fake GitHub running on this computer. The clock is fixed at
//! 2026-10-26 12:00 UTC, so the date ranges the wisp asks for are always the same.

mod common;

use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use common::{ok, start, status, Route};
use rusqlite::Connection;
use wisply_core::http::Client;
use wisply_core::scheduler::Fetcher;
use wisply_core::wisps::github::{self, compact, GithubFetcher};
use wisply_core::{schema, store};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap()
}

/// One repo as GitHub's search would describe it.
fn repo(id: i64, name: &str, stars: i64, created: &str, description: Option<&str>, language: Option<&str>) -> String {
    let text = |v: Option<&str>| match v {
        Some(v) => serde_json::to_string(v).unwrap(),
        None => "null".to_string(),
    };
    format!(
        r#"{{"id": {id}, "full_name": "{name}", "stargazers_count": {stars}, "html_url": "https://github.com/{name}", "created_at": "{created}", "description": {}, "language": {}}}"#,
        text(description),
        text(language)
    )
}

fn list(repos: &[String]) -> String {
    format!(r#"{{"total_count": {}, "items": [{}]}}"#, repos.len(), repos.join(","))
}

/// Routes for the five searches. The needles pick the search by what it asks for.
fn routes(month: &str, week: &str, day: &str, gems: &str, langs: &str) -> Vec<Route> {
    vec![
        ok("~2026-09-26&sort", month),
        ok("~2026-10-19&sort", week),
        ok("~2026-10-25&sort", day),
        ok("~stars%3A50..500", gems),
        ok("~language%3A", langs),
    ]
}

fn setup(routes: Vec<Route>) -> (Connection, GithubFetcher, common::Server) {
    let server = start(routes);
    let conn = schema::open_in_memory().unwrap();
    github::register_wisp(&conn).unwrap();
    store::set_wisp_active(&conn, "github", true).unwrap();
    let url = format!("{}/search/repositories", server.base);
    let fetcher = GithubFetcher::with_client(Client::insecure_for_tests(), &url, Duration::ZERO).with_now(now());
    (conn, fetcher, server)
}

fn texts(conn: &Connection, channel: &str) -> Vec<String> {
    store::items(conn, channel, None).unwrap().into_iter().map(|i| i.text).collect()
}

#[test]
fn registering_creates_the_wisp_and_its_six_channels_in_order() {
    let conn = schema::open_in_memory().unwrap();
    github::register_wisp(&conn).unwrap();
    github::register_wisp(&conn).unwrap();
    let wisp = store::get_wisp(&conn, "github").unwrap().unwrap();
    assert_eq!((wisp.name.as_str(), wisp.color.as_str()), ("Gitty", "#8957e5"));
    let ids: Vec<_> = store::wisp_channels(&conn, "github").unwrap().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, ["gh_day", "gh_week", "gh_month", "gh_rising", "gh_gems", "gh_langs"]);
}

#[test]
fn star_counts_are_shortened() {
    for (number, shown) in [
        (0, "0"),
        (845, "845"),
        (999, "999"),
        (1000, "1k"),
        (1250, "1.2k"),
        (4816, "4.8k"),
        (9999, "10k"),
        (10_000, "10k"),
        (10_500, "10k"),
        (11_500, "12k"),
        (45_979, "46k"),
        (999_400, "999k"),
        (999_999, "1M"),
        (1_000_000, "1M"),
        (1_250_000, "1.2M"),
        (12_340_000, "12.3M"),
    ] {
        assert_eq!(compact(number), shown, "for {number}");
    }
}

#[test]
fn a_check_fills_every_channel_with_the_right_repos() {
    let month = list(&[
        repo(1, "o/big", 1000, "2026-10-06T12:00:00Z", Some("A big one"), Some("Rust")),
        repo(2, "o/fast", 600, "2026-10-24T12:00:00Z", None, None),
        repo(3, "o/new", 90, "2026-10-26T06:00:00Z", None, None),
    ]);
    let week = list(&[repo(4, "o/week", 300, "2026-10-20T00:00:00Z", None, Some("Go")), repo(5, "o/week2", 200, "2026-10-21T00:00:00Z", None, None)]);
    let day = list(&[repo(6, "o/day", 50, "2026-10-26T01:00:00Z", Some("Today's"), None)]);
    let gems = list(&[
        repo(7, "o/gem", 120, "2026-10-10T00:00:00Z", Some("Has a description"), None),
        repo(8, "o/mute", 130, "2026-10-10T00:00:00Z", None, None),
        repo(9, "o/gem2", 110, "2026-10-11T00:00:00Z", Some("Another"), None),
    ]);
    let langs = list(&[repo(10, "o/py", 70, "2026-10-22T00:00:00Z", None, Some("Python")), repo(11, "o/py2", 90, "2026-10-22T00:00:00Z", None, Some("Python"))]);
    let (conn, mut fetcher, _server) = setup(routes(&month, &week, &day, &gems, &langs));
    fetcher.fetch(&conn).unwrap();

    // Newest first: the best repo was added last, so it is listed first.
    assert_eq!(texts(&conn, "gh_month"), ["o/big  (★ 1k, Rust) - A big one", "o/fast  (★ 600)", "o/new  (★ 90)"]);
    // Rising: 600 stars in 2 days = 300 a day; 90 stars in under a day counts as 1 day = 90; 1000 in 20 days = 50.
    assert_eq!(
        texts(&conn, "gh_rising"),
        ["o/fast  (★ 600, ~300/day)", "o/new  (★ 90, ~90/day)", "o/big  (★ 1k, ~50/day, Rust) - A big one"]
    );
    assert_eq!(texts(&conn, "gh_week"), ["o/week  (★ 300, Go)", "o/week2  (★ 200)"]);
    assert_eq!(texts(&conn, "gh_day"), ["o/day  (★ 50) - Today's"]);
    // Hidden gems skip repos with no description.
    assert_eq!(texts(&conn, "gh_gems"), ["o/gem  (★ 120) - Has a description", "o/gem2  (★ 110) - Another"]);
    // My languages: best stars first.
    assert_eq!(texts(&conn, "gh_langs"), ["o/py2  (★ 90, Python)", "o/py  (★ 70, Python)"]);

    let first = &store::items(&conn, "gh_month", None).unwrap()[0];
    assert_eq!(first.stars, Some(1000));
    assert_eq!(first.url.as_deref(), Some("https://github.com/o/big"));
    assert_eq!(first.discount_percent, None);
    assert!(store::has_item(&conn, "gh_month:1").unwrap());
    assert!(store::has_item(&conn, "gh_rising:1").unwrap());
}

const EMPTY: &str = r#"{"items": []}"#;

#[test]
fn the_right_searches_are_asked_for() {
    let (conn, mut fetcher, server) = setup(routes(EMPTY, EMPTY, EMPTY, EMPTY, EMPTY));
    fetcher.fetch(&conn).unwrap();
    let asked: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert_eq!(asked.len(), 5);
    assert!(asked[0].contains("q=created%3A%3E%3D2026-09-26&sort=stars&order=desc&per_page=100"), "{}", asked[0]);
    assert!(asked[1].contains("2026-10-19&sort=stars&order=desc&per_page=5"), "{}", asked[1]);
    assert!(asked[2].contains("2026-10-25&sort=stars&order=desc&per_page=5"), "{}", asked[2]);
    assert!(asked[3].contains("2026-09-26+stars%3A50..500+archived%3Afalse&sort=stars&order=desc&per_page=30"), "{}", asked[3]);
    assert!(asked[4].contains("2026-10-19+language%3A%22python%22&sort=stars&order=desc&per_page=5"), "{}", asked[4]);
}

#[test]
fn the_request_says_who_is_asking_and_which_api() {
    let (conn, mut fetcher, server) = setup(routes(EMPTY, EMPTY, EMPTY, EMPTY, EMPTY));
    fetcher.fetch(&conn).unwrap();
    let first = &server.requests()[0];
    assert_eq!(first.header("accept"), Some("application/vnd.github+json"));
    assert_eq!(first.header("x-github-api-version"), Some("2022-11-28"));
    assert!(first.header("user-agent").unwrap().starts_with("wisply/"));
}

#[test]
fn checking_again_adds_nothing_and_dismissed_repos_stay_gone() {
    let month = list(&[repo(1, "o/a", 100, "2026-10-20T00:00:00Z", None, None), repo(2, "o/b", 90, "2026-10-20T00:00:00Z", None, None)]);
    let (conn, mut fetcher, _server) = setup(routes(&month, EMPTY, EMPTY, EMPTY, EMPTY));
    fetcher.fetch(&conn).unwrap();
    assert_eq!(texts(&conn, "gh_month").len(), 2);
    let newest = store::items(&conn, "gh_month", None).unwrap()[0].id;
    store::dismiss_item(&conn, newest).unwrap();
    fetcher.fetch(&conn).unwrap();
    assert_eq!(texts(&conn, "gh_month"), ["o/b  (★ 90)"]);
}

#[test]
fn only_the_top_five_are_kept() {
    let many: Vec<String> =
        (1..=8).map(|n| repo(n, &format!("o/r{n}"), 100 - n, "2026-10-20T00:00:00Z", None, None)).collect();
    let (conn, mut fetcher, _server) = setup(routes(&list(&many), EMPTY, EMPTY, EMPTY, EMPTY));
    fetcher.fetch(&conn).unwrap();
    assert_eq!(texts(&conn, "gh_month").len(), 5);
    assert_eq!(texts(&conn, "gh_month")[0], "o/r1  (★ 99)");
    assert_eq!(texts(&conn, "gh_rising").len(), 5);
}

#[test]
fn long_descriptions_are_cut_and_odd_text_is_cleaned() {
    let long = "x".repeat(80);
    let month = list(&[
        repo(1, "o/long", 10, "2026-10-20T00:00:00Z", Some(&long), None),
        repo(2, "o/odd", 9, "2026-10-20T00:00:00Z", Some("line one\nline two \u{202E} tricks"), Some("C++")),
    ]);
    let (conn, mut fetcher, _server) = setup(routes(&month, EMPTY, EMPTY, EMPTY, EMPTY));
    fetcher.fetch(&conn).unwrap();
    let got = texts(&conn, "gh_month");
    assert_eq!(got[0], format!("o/long  (★ 10) - {}...", "x".repeat(49)));
    assert_eq!(got[1], "o/odd  (★ 9, C++) - line one line two tricks");
}

#[test]
fn links_only_survive_if_they_point_at_github() {
    let good = repo(1, "o/good", 5, "2026-10-20T00:00:00Z", None, None);
    let bad = good
        .replace("o/good", "o/bad")
        .replace(r#""id": 1"#, r#""id": 2"#)
        .replace("https://github.com/o/bad", "https://evil.example/o/bad");
    let worse = good
        .replace("o/good", "o/worse")
        .replace(r#""id": 1"#, r#""id": 3"#)
        .replace("https://github.com/o/worse", "javascript:alert(1)");
    let (conn, mut fetcher, _server) = setup(routes(&list(&[good, bad, worse]), EMPTY, EMPTY, EMPTY, EMPTY));
    fetcher.fetch(&conn).unwrap();
    let urls: Vec<_> = store::items(&conn, "gh_month", None).unwrap().into_iter().map(|i| i.url).collect();
    assert_eq!(urls, [Some("https://github.com/o/good".to_string()), None, None]);
}

#[test]
fn repos_missing_the_essentials_are_skipped_not_fatal() {
    let month = r#"{"items": [
        {"id": 1, "full_name": "o/ok", "stargazers_count": 5, "html_url": "https://github.com/o/ok", "created_at": "2026-10-20T00:00:00Z"},
        {"full_name": "o/no-id", "stargazers_count": 5, "html_url": "https://github.com/o/no-id"},
        {"id": 3, "stargazers_count": 5, "html_url": "https://github.com/o/no-name"},
        {"id": 4, "full_name": "o/no-stars", "html_url": "https://github.com/o/no-stars"},
        "not even an object",
        {"id": 5, "full_name": "o/bad-date", "stargazers_count": 7, "html_url": "https://github.com/o/bad-date", "created_at": "yesterday"}
    ]}"#;
    let (conn, mut fetcher, _server) = setup(routes(month, EMPTY, EMPTY, EMPTY, EMPTY));
    fetcher.fetch(&conn).unwrap();
    assert_eq!(texts(&conn, "gh_month"), ["o/ok  (★ 5)", "o/bad-date  (★ 7)"]);
    // A repo whose creation date can't be read counts as 0 stars a day, so it comes last in Rising.
    assert_eq!(texts(&conn, "gh_rising")[1], "o/bad-date  (★ 7, ~0/day)");
}

#[test]
fn a_refusal_names_the_reset_time_and_stops_the_check() {
    let mut refused = status("/search/repositories", 403, r#"{"message": "rate limit"}"#);
    refused.headers.push(("x-ratelimit-reset", "1793019600".to_string()));
    let (conn, mut fetcher, _server) = setup(vec![refused]);
    assert_eq!(
        fetcher.fetch(&conn).unwrap_err(),
        "GitHub refused the request (status 403, limit resets at epoch 1793019600)"
    );
    assert!(store::items(&conn, "gh_month", None).unwrap().is_empty());

    let (conn, mut fetcher, _server) = setup(vec![status("/search/repositories", 429, "slow down")]);
    assert_eq!(
        fetcher.fetch(&conn).unwrap_err(),
        "GitHub refused the request (status 429, limit resets at epoch unknown)"
    );
}

#[test]
fn a_failure_part_way_keeps_what_was_already_saved() {
    let month = list(&[repo(1, "o/a", 5, "2026-10-20T00:00:00Z", None, None)]);
    let (conn, mut fetcher, _server) = setup(vec![ok("~2026-09-26&sort", &month), status("/search/repositories", 500, "oops")]);
    assert_eq!(fetcher.fetch(&conn).unwrap_err(), "GitHub answered with status 500");
    assert_eq!(texts(&conn, "gh_month").len(), 1);
    assert!(texts(&conn, "gh_week").is_empty());
}

#[test]
fn a_reply_that_is_not_json_or_has_a_bad_list_is_an_error() {
    let (conn, mut fetcher, _server) = setup(vec![ok("/search/repositories", "<html>hi</html>")]);
    assert_eq!(fetcher.fetch(&conn).unwrap_err(), "GitHub sent something that isn't JSON");
    let (conn, mut fetcher, _server) = setup(vec![ok("/search/repositories", r#"{"items": "nope"}"#)]);
    assert_eq!(fetcher.fetch(&conn).unwrap_err(), "GitHub's list of repositories was not a list");
    // No "items" at all just means nothing found.
    let (conn, mut fetcher, _server) = setup(vec![ok("/search/repositories", "{}")]);
    fetcher.fetch(&conn).unwrap();
}

#[test]
fn several_languages_are_merged_without_repeats() {
    let langs = list(&[repo(1, "o/shared", 40, "2026-10-22T00:00:00Z", None, Some("Go"))]);
    let (conn, fetcher, server) = setup(routes(EMPTY, EMPTY, EMPTY, EMPTY, &langs));
    let mut fetcher = fetcher.with_languages(&["go", "rust"]);
    fetcher.fetch(&conn).unwrap();
    assert_eq!(texts(&conn, "gh_langs"), ["o/shared  (★ 40, Go)"]);
    let asked: Vec<String> = server.requests().into_iter().map(|r| r.target).collect();
    assert!(asked.iter().any(|t| t.contains("language%3A%22go%22")));
    assert!(asked.iter().any(|t| t.contains("language%3A%22rust%22")));
}

#[test]
fn the_real_client_only_talks_https_to_the_listed_site() {
    let (conn, _fetcher, server) = setup(vec![ok("/search/repositories", EMPTY)]);
    let url = format!("{}/search/repositories", server.base);
    let mut strict = GithubFetcher::with_client(Client::strict(&["api.github.com"]), &url, Duration::ZERO);
    let error = strict.fetch(&conn).unwrap_err();
    assert!(error.contains("only https is allowed"), "{error}");
    assert!(server.requests().is_empty());
}

#[test]
fn resetting_removes_only_this_wisps_items() {
    let (conn, mut fetcher, _server) = setup(routes(
        &list(&[repo(1, "o/a", 5, "2026-10-20T00:00:00Z", None, None)]),
        EMPTY,
        EMPTY,
        EMPTY,
        EMPTY,
    ));
    fetcher.fetch(&conn).unwrap();
    store::add_wisp(&conn, "other", "Other", "#000").unwrap();
    store::upsert_channel(&conn, "oc", "other", "T", "D", "#000", 0).unwrap();
    for key in ["ghX:1", "gh:1", "plain"] {
        let item = store::NewItem { channel_id: "oc".into(), dedupe_key: key.into(), text: key.into(), ..Default::default() };
        store::add_item(&conn, &item).unwrap();
    }
    assert_eq!(github::reset_items(&conn).unwrap(), 2); // gh_month:1 and gh_rising:1
    assert!(texts(&conn, "gh_month").is_empty());
    assert_eq!(store::items(&conn, "oc", None).unwrap().len(), 3);
    // After a reset the same repos are stored again.
    fetcher.fetch(&conn).unwrap();
    assert_eq!(texts(&conn, "gh_month").len(), 1);
}

#[test]
fn a_gap_between_searches_really_waits() {
    let (conn, _unused, server) = setup(routes(EMPTY, EMPTY, EMPTY, EMPTY, EMPTY));
    let url = format!("{}/search/repositories", server.base);
    let mut slow =
        GithubFetcher::with_client(Client::insecure_for_tests(), &url, Duration::from_millis(100)).with_now(now());
    let started = std::time::Instant::now();
    slow.fetch(&conn).unwrap();
    // Four pauses: before the week, day, gems and the one language search.
    assert!(started.elapsed() >= Duration::from_millis(400), "{:?}", started.elapsed());
}