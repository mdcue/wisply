//! Tests for what the sources say about waiting: reading it from a reply, and the "check now"
//! button respecting it. Replies come from a fake website on this computer.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use common::{start, status};
use rusqlite::Connection;
use wisply_core::http::{Client, Reply};
use wisply_core::hub::Hub;
use wisply_core::scheduler::Fetcher;
use wisply_core::{paths, schema, store};

/// The fake site answers `code` with these extra headers, and we hand back what the client saw.
fn reply_with(code: u16, headers: &[(&'static str, &str)]) -> Reply {
    let mut route = status("/x", code, "no");
    for (name, value) in headers {
        route.headers.push((*name, value.to_string()));
    }
    let server = start(vec![route]);
    let url = url::Url::parse(&format!("{}/x", server.base)).unwrap();
    Client::insecure_for_tests().get(&url, &[]).unwrap()
}

const NOW: i64 = 1_000;

#[test]
fn retry_after_in_seconds_is_the_wait() {
    assert_eq!(reply_with(429, &[("Retry-After", "120")]).wait_seconds(NOW), Some(120));
    assert_eq!(reply_with(429, &[("retry-after", " 45 ")]).wait_seconds(NOW), Some(45));
}

#[test]
fn retry_after_as_a_date_is_counted_from_now() {
    // 00:20:00 on 1 January 1970 is 1200 seconds after the start of unix time.
    let reply = reply_with(503, &[("Retry-After", "Thu, 01 Jan 1970 00:20:00 GMT")]);
    assert_eq!(reply.wait_seconds(NOW), Some(200));
    // A date that has already passed means no wait left.
    assert_eq!(reply.wait_seconds(5_000), Some(0));
}

#[test]
fn an_empty_quota_waits_until_the_reset_time() {
    let headers = [("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "1500")];
    assert_eq!(reply_with(403, &headers).wait_seconds(NOW), Some(500));
    assert_eq!(reply_with(403, &headers).wait_seconds(9_999), Some(0));
}

#[test]
fn retry_after_wins_over_the_reset_time() {
    let headers = [("Retry-After", "30"), ("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "1500")];
    assert_eq!(reply_with(403, &headers).wait_seconds(NOW), Some(30));
}

#[test]
fn a_quota_that_is_not_empty_or_no_hint_means_no_answer() {
    let some_left = [("x-ratelimit-remaining", "7"), ("x-ratelimit-reset", "1500")];
    assert_eq!(reply_with(403, &some_left).wait_seconds(NOW), None);
    assert_eq!(reply_with(500, &[]).wait_seconds(NOW), None);
    // A reset time with no "remaining" header is not taken as a request to wait.
    assert_eq!(reply_with(403, &[("x-ratelimit-reset", "1500")]).wait_seconds(NOW), None);
}

#[test]
fn nonsense_headers_are_ignored() {
    assert_eq!(reply_with(429, &[("Retry-After", "soon")]).wait_seconds(NOW), None);
    let odd = [("x-ratelimit-remaining", "0"), ("x-ratelimit-reset", "tomorrow")];
    assert_eq!(reply_with(403, &odd).wait_seconds(NOW), None);
    // A negative number never turns into a negative wait.
    assert_eq!(reply_with(429, &[("Retry-After", "-5")]).wait_seconds(NOW), Some(0));
}

/// Always fails, and says (or doesn't say) how long the source asked us to wait.
struct Limited {
    calls: Arc<AtomicUsize>,
    ask: Option<i64>,
}

impl Fetcher for Limited {
    fn wisp_id(&self) -> &str {
        "w"
    }
    fn fetch(&mut self, _conn: &Connection) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err("slow down".to_string())
    }
    fn retry_wait_seconds(&self) -> Option<i64> {
        self.ask
    }
}

fn hub_with_wisp() -> (tempfile::TempDir, Hub) {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = schema::open(&dir.path().join(paths::DB_FILE)).unwrap();
        store::add_wisp(&conn, "w", "W", "#000").unwrap();
    }
    let hub = Hub::new(dir.path());
    (dir, hub)
}

#[test]
fn a_source_that_asked_for_time_is_not_asked_again_by_check_now() {
    let (_dir, hub) = hub_with_wisp();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut fetcher = Limited { calls: calls.clone(), ask: Some(3600) };

    let first = hub.fetch_with(&mut fetcher).unwrap();
    assert!(!first.ok);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // The second press is answered without touching the source.
    let second = hub.fetch_with(&mut fetcher).unwrap();
    assert!(!second.ok);
    assert_eq!(second.new_items, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn an_ordinary_failure_does_not_block_check_now() {
    let (_dir, hub) = hub_with_wisp();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut fetcher = Limited { calls: calls.clone(), ask: None };
    hub.fetch_with(&mut fetcher).unwrap();
    hub.fetch_with(&mut fetcher).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn a_wait_that_has_already_run_out_does_not_block() {
    let (_dir, hub) = hub_with_wisp();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut fetcher = Limited { calls: calls.clone(), ask: Some(0) };
    hub.fetch_with(&mut fetcher).unwrap();
    hub.fetch_with(&mut fetcher).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}