//! Tests for the pop-up's open and close rules. Time is a plain number, so these never wait.

use wisply_core::popup_guard::{Click, Guard, IGNORE_BLUR_AFTER_SHOWING_MS, IGNORE_CLICK_AFTER_BLUR_MS};

const ICON: (i32, i32) = (1800, 1040);

#[test]
fn the_first_click_asks_the_page_to_refresh_and_remembers_where() {
    let mut guard = Guard::new();
    assert_eq!(guard.tray_clicked(1000, false, ICON), Click::Open { ticket: 1 });
    assert_eq!(guard.anchor(), ICON);
}

#[test]
fn a_click_while_it_is_open_closes_it() {
    let mut guard = Guard::new();
    guard.tray_clicked(1000, false, ICON);
    assert!(guard.page_ready(1100));
    assert_eq!(guard.tray_clicked(5000, true, ICON), Click::Hide);
}

#[test]
fn it_is_shown_once_when_the_page_is_ready() {
    let mut guard = Guard::new();
    guard.tray_clicked(1000, false, ICON);
    assert!(guard.page_ready(1100));
    assert!(!guard.page_ready(1200), "a later resize must not show it again");
}

#[test]
fn a_page_that_is_ready_without_being_asked_does_not_show_it() {
    let mut guard = Guard::new();
    assert!(!guard.page_ready(1000));
}

#[test]
fn it_is_shown_anyway_if_the_page_never_answers_but_not_for_an_old_request() {
    let mut guard = Guard::new();
    let Click::Open { ticket: first } = guard.tray_clicked(1000, false, ICON) else { panic!() };
    let Click::Open { ticket: second } = guard.tray_clicked(1100, false, ICON) else { panic!() };
    assert!(!guard.show_anyway(first, 2000), "the first request has been replaced");
    assert!(guard.show_anyway(second, 2000));
    assert!(!guard.show_anyway(second, 2100), "already shown");
}

#[test]
fn the_wait_does_nothing_once_the_page_has_answered() {
    let mut guard = Guard::new();
    let Click::Open { ticket } = guard.tray_clicked(1000, false, ICON) else { panic!() };
    assert!(guard.page_ready(1100));
    assert!(!guard.show_anyway(ticket, 2000));
}

#[test]
fn losing_focus_right_after_showing_is_ignored_but_later_it_closes() {
    let mut guard = Guard::new();
    guard.tray_clicked(1000, false, ICON);
    guard.page_ready(1100);
    assert!(!guard.focus_lost(1100 + IGNORE_BLUR_AFTER_SHOWING_MS - 1, true));
    assert!(guard.focus_lost(1100 + IGNORE_BLUR_AFTER_SHOWING_MS, true));
}

#[test]
fn losing_focus_while_hidden_or_still_waiting_changes_nothing() {
    let mut guard = Guard::new();
    assert!(!guard.focus_lost(1000, false));
    guard.tray_clicked(2000, false, ICON);
    assert!(!guard.focus_lost(3000, true), "still waiting for the page");
    // and it did not count as a close, so the next click opens normally
    assert!(matches!(guard.tray_clicked(3001, false, ICON), Click::Open { .. }));
}

#[test]
fn the_click_that_caused_the_close_is_ignored_but_a_later_one_opens_it_again() {
    let mut guard = Guard::new();
    guard.tray_clicked(1000, false, ICON);
    guard.page_ready(1100);
    assert!(guard.focus_lost(9000, true));
    assert_eq!(guard.tray_clicked(9000 + IGNORE_CLICK_AFTER_BLUR_MS - 1, false, ICON), Click::Ignore);
    assert!(matches!(guard.tray_clicked(9000 + IGNORE_CLICK_AFTER_BLUR_MS, false, ICON), Click::Open { .. }));
}

#[test]
fn closing_by_escape_stops_the_wait() {
    let mut guard = Guard::new();
    let Click::Open { ticket } = guard.tray_clicked(1000, false, ICON) else { panic!() };
    guard.closed();
    assert!(!guard.page_ready(1100));
    assert!(!guard.show_anyway(ticket, 2000));
}

#[test]
fn a_new_click_moves_the_anchor() {
    let mut guard = Guard::new();
    guard.tray_clicked(1000, false, ICON);
    guard.closed();
    guard.tray_clicked(9000, false, (-300, 20));
    assert_eq!(guard.anchor(), (-300, 20));
}

#[test]
fn nothing_overflows_with_odd_times() {
    let mut guard = Guard::new();
    guard.tray_clicked(u64::MAX, false, ICON);
    guard.page_ready(0);
    let _ = guard.focus_lost(0, true);
    guard.tray_clicked(0, false, ICON);
}