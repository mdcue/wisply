//! Tests for cleaning outside text and checking links.

use wisply_core::safe::{clean_text, link_on_hosts};

#[test]
fn line_breaks_and_extra_spaces_become_single_spaces() {
    assert_eq!(clean_text("  a\n\nb\t\tc   d \r\n", 100), "a b c d");
}

#[test]
fn control_and_text_direction_characters_are_removed() {
    assert_eq!(clean_text("ab\u{0}c\u{7}d", 100), "ab c d");
    assert_eq!(clean_text("gam\u{202E}e\u{2066}s\u{200F}", 100), "games");
}

#[test]
fn long_text_is_cut_with_an_ellipsis() {
    let long = "x".repeat(50);
    let cut = clean_text(&long, 10);
    assert_eq!(cut.chars().count(), 10);
    assert!(cut.ends_with('…'));
    assert_eq!(clean_text("short", 10), "short");
    assert_eq!(clean_text("exactly ten", 11), "exactly ten");
}

#[test]
fn cutting_never_splits_a_character() {
    let cut = clean_text("ééééééééé", 5);
    assert_eq!(cut, "éééé…");
}

#[test]
fn only_https_links_to_listed_hosts_are_kept() {
    let hosts = ["github.com", "www.cheapshark.com"];
    assert!(link_on_hosts("https://github.com/owner/repo", &hosts).is_some());
    assert!(link_on_hosts("  https://GitHub.com/owner/repo  ", &hosts).is_some());
    assert!(link_on_hosts("https://www.cheapshark.com/redirect?dealID=1", &hosts).is_some());

    for bad in [
        "http://github.com/owner/repo",
        "javascript:alert(1)",
        "file:///C:/Windows/System32/calc.exe",
        "https://evil.example/github.com",
        "https://github.com.evil.example/x",
        "https://user:pass@github.com/x",
        "https://github.com@evil.example/x",
        "ftp://github.com/x",
        "github.com/owner/repo",
        "",
    ] {
        assert_eq!(link_on_hosts(bad, &hosts), None, "should be refused: {bad:?}");
    }
}