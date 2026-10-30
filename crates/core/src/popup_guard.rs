//! The rules for when the tray pop-up opens and closes, kept apart from the window itself so they
//! can be tested. The window code (in `src-tauri`) asks this what to do and does it.
//!
//! Times are milliseconds on any steadily running clock; only the differences matter.
//!
//! The rules exist because Windows sends events in an awkward order:
//! * Clicking the tray icon while the pop-up is open first takes the focus away from the pop-up
//!   (which closes it), and only then delivers the click. Without a rule, that click would open it
//!   again straight away, so a click just after a close-by-losing-focus is ignored.
//! * Showing a window can make it lose focus for a moment. A focus loss right after showing is ignored.
//! * The page needs a moment to draw the newest data. The pop-up is shown once the page says it is
//!   ready, or after a short wait if it never says so.

/// A loss of focus this soon after showing is not a real "clicked elsewhere".
pub const IGNORE_BLUR_AFTER_SHOWING_MS: u64 = 250;
/// A tray click this soon after the pop-up closed itself by losing focus was the click that closed it.
pub const IGNORE_CLICK_AFTER_BLUR_MS: u64 = 300;
/// How long to wait for the page before showing the pop-up anyway.
pub const SHOW_ANYWAY_AFTER_MS: u64 = 900;

/// What to do about a tray click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// The pop-up is open: close it.
    Hide,
    /// Do nothing (this click is the one that just closed the pop-up).
    Ignore,
    /// Ask the page to refresh itself; it is shown when the page is ready. If the page has not
    /// answered after `SHOW_ANYWAY_AFTER_MS`, call `show_anyway` with this ticket.
    Open { ticket: u64 },
}

#[derive(Debug, Default)]
pub struct Guard {
    anchor: (i32, i32),
    waiting: bool,
    ticket: u64,
    shown_at: Option<u64>,
    blurred_at: Option<u64>,
}

impl Guard {
    pub fn new() -> Guard {
        Guard::default()
    }

    /// Where the pop-up hangs from: the top-left of the tray icon that was clicked (physical pixels).
    pub fn anchor(&self) -> (i32, i32) {
        self.anchor
    }

    /// The tray icon was clicked. `visible` is whether the pop-up is on screen now.
    pub fn tray_clicked(&mut self, now: u64, visible: bool, anchor: (i32, i32)) -> Click {
        if visible {
            self.waiting = false;
            return Click::Hide;
        }
        if self.blurred_at.is_some_and(|at| now.saturating_sub(at) < IGNORE_CLICK_AFTER_BLUR_MS) {
            return Click::Ignore;
        }
        self.anchor = anchor;
        self.waiting = true;
        self.ticket += 1;
        Click::Open { ticket: self.ticket }
    }

    /// The page has drawn itself. True if the pop-up should be shown now (only when one was asked for).
    pub fn page_ready(&mut self, now: u64) -> bool {
        if !self.waiting {
            return false;
        }
        self.waiting = false;
        self.shown_at = Some(now);
        true
    }

    /// The wait for the page ran out. True if the pop-up should be shown now: that open request is
    /// still the latest one and the page never answered.
    pub fn show_anyway(&mut self, ticket: u64, now: u64) -> bool {
        ticket == self.ticket && self.page_ready(now)
    }

    /// The pop-up lost the focus. True if it should close.
    pub fn focus_lost(&mut self, now: u64, visible: bool) -> bool {
        if !visible || self.waiting {
            return false;
        }
        if self.shown_at.is_some_and(|at| now.saturating_sub(at) < IGNORE_BLUR_AFTER_SHOWING_MS) {
            return false;
        }
        self.blurred_at = Some(now);
        true
    }

    /// The pop-up was closed some other way (Escape, a button): stop waiting for the page.
    pub fn closed(&mut self) {
        self.waiting = false;
    }
}