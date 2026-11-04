//! Hostile input: text and links a website could send, run through the same cleaning every wisp
//! uses before anything is stored or shown. The pages show item text with `textContent` (never as
//! HTML), so for text the job here is: no hidden characters, no huge strings. For links the job is:
//! only `https` links to a listed host survive.

use wisply_core::safe::{clean_text, link_on_hosts, MAX_ITEM_TEXT, MAX_LINK_LEN};

const HOSTS: &[&str] = &["github.com", "www.cheapshark.com"];

#[test]
fn script_like_text_is_kept_as_plain_characters() {
    // Nothing is "escaped" here; it stays as text because the pages never treat it as HTML.
    let nasty = "<img src=x onerror=alert(1)> <script>alert(2)</script>";
    assert_eq!(clean_text(nasty, MAX_ITEM_TEXT), nasty);
}

#[test]
fn control_characters_and_line_breaks_become_single_spaces() {
    assert_eq!(clean_text("a\u{0}b\r\nc\td\u{7}e", MAX_ITEM_TEXT), "a b c d e");
}

#[test]
fn text_direction_tricks_are_removed() {
    assert_eq!(clean_text("invoice\u{202E}fdp.exe", MAX_ITEM_TEXT), "invoicefdp.exe");
    assert_eq!(clean_text("a\u{200F}b\u{2067}c\u{2069}d", MAX_ITEM_TEXT), "abcd");
}

#[test]
fn invisible_characters_are_removed() {
    assert_eq!(clean_text("pay\u{200B}pal\u{FEFF}.com\u{2060}", MAX_ITEM_TEXT), "paypal.com");
    assert_eq!(clean_text("soft\u{00AD}hyphen", MAX_ITEM_TEXT), "softhyphen");
}

#[test]
fn emoji_joiners_survive() {
    let family = "👨\u{200D}👩\u{200D}👧";
    assert_eq!(clean_text(family, MAX_ITEM_TEXT), family);
}

#[test]
fn very_long_text_is_cut_to_the_limit() {
    let cut = clean_text(&"x".repeat(100_000), MAX_ITEM_TEXT);
    assert_eq!(cut.chars().count(), MAX_ITEM_TEXT);
    assert!(cut.ends_with('…'));
}

#[test]
fn cutting_never_splits_a_multi_byte_character() {
    for text in ["é".repeat(1_000), "😀".repeat(1_000), "日本語".repeat(500)] {
        let cut = clean_text(&text, MAX_ITEM_TEXT);
        assert_eq!(cut.chars().count(), MAX_ITEM_TEXT);
    }
}

#[test]
fn empty_and_blank_text_give_nothing() {
    assert_eq!(clean_text("", MAX_ITEM_TEXT), "");
    assert_eq!(clean_text(" \t\r\n\u{200B} ", MAX_ITEM_TEXT), "");
}

#[test]
fn sql_looking_text_is_just_text() {
    // Safe because every query passes values as bound parameters, not because of this cleaning.
    let text = "'); DROP TABLE items; --";
    assert_eq!(clean_text(text, MAX_ITEM_TEXT), text);
}

#[test]
fn good_links_are_kept() {
    assert_eq!(
        link_on_hosts("https://github.com/rust-lang/rust", HOSTS).as_deref(),
        Some("https://github.com/rust-lang/rust")
    );
    assert_eq!(
        link_on_hosts("  https://www.cheapshark.com/redirect?dealID=abc  ", HOSTS).as_deref(),
        Some("https://www.cheapshark.com/redirect?dealID=abc")
    );
    assert!(link_on_hosts("https://GITHUB.com/x", HOSTS).is_some());
}

#[test]
fn dangerous_or_foreign_links_are_dropped() {
    let bad = [
        // not https
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        " javascript:alert(1)",
        "vbscript:msgbox(1)",
        "data:text/html,<script>alert(1)</script>",
        "file:///C:/Windows/System32/calc.exe",
        "ms-msdt:/id PCWDiagnostic",
        "mailto:a@github.com",
        "ftp://github.com/x",
        "http://github.com/x",
        // not even a full address
        "//github.com/x",
        "/relative/path",
        "github.com/x",
        "",
        "   ",
        "not a url",
        // https, but not a listed host (or tricks to look like one)
        "https://evil.example/github.com",
        "https://github.com.evil.example/",
        "https://evilgithub.com/",
        "https://github.com@evil.example/",
        "https://github.com:pw@evil.example/",
        "https://user:pw@github.com/x",
        "https://evil.example\\@github.com",
        "https://github.com%2eevil.example/",
        "https://gıthub.com/x",
    ];
    for link in bad {
        assert_eq!(link_on_hosts(link, HOSTS), None, "should be refused: {link}");
    }
}

#[test]
fn absurdly_long_links_are_dropped() {
    let long = format!("https://github.com/{}", "a".repeat(MAX_LINK_LEN));
    assert_eq!(link_on_hosts(&long, HOSTS), None);
    let fine = format!("https://github.com/{}", "a".repeat(100));
    assert!(link_on_hosts(&fine, HOSTS).is_some());
}