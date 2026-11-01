//! The tray pop-up: one small always-on-top window with two views (a strip of the active wisps, then
//! one wisp's panel). The page is `ui/popup.html`. When it opens, closes or lands on screen is decided
//! by `wisply_core::popup_guard` (tested); this file only does what that says.
//!
//! The window is made once at start-up and kept hidden, so it opens at once. To open it, the page is
//! told to redraw (`popup-open`); when it has drawn itself it reports its size (`popup_ready`), and
//! only then is the window placed beside the tray icon and shown, so you never see a half-drawn pop-up.

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Rect, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
use wisply_core::placement::{self, Area};
use wisply_core::popup_guard::{Click, Guard, SHOW_ANYWAY_AFTER_MS};

pub const LABEL: &str = "popup";
const START_WIDTH: f64 = 300.0;
const START_HEIGHT: f64 = 120.0;

/// The pop-up's shared memory: what the rules in `popup_guard` need to remember between events.
#[derive(Default)]
pub struct PopupState(Mutex<Guard>);

fn guard(state: &PopupState) -> MutexGuard<'_, Guard> {
    state.0.lock().unwrap_or_else(|e| e.into_inner())
}

/// Milliseconds since the app started (a steady clock, which is all the rules need).
fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

/// Makes the (hidden) pop-up window. Call once, when the app starts.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("popup.html".into()))
        .title("wisply")
        .inner_size(START_WIDTH, START_HEIGHT)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Focused(false) => lost_focus(&handle),
        // Alt+F4 and the like only hide it; the window is needed again at the next click.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            hide(&handle);
        }
        _ => {}
    });
    Ok(())
}

/// Where the pop-up hangs from: the top-left corner of the tray icon that was clicked.
pub fn anchor_of(rect: &Rect, click: PhysicalPosition<f64>) -> (i32, i32) {
    match rect.position {
        Position::Physical(p) => (p.x, p.y),
        Position::Logical(_) => (click.x.round() as i32, click.y.round() as i32),
    }
}

/// A click on the tray icon: opens the pop-up, or closes it if it is open.
pub fn toggle(app: &AppHandle, anchor: (i32, i32)) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let visible = window.is_visible().unwrap_or(false);
    let click = guard(&app.state::<PopupState>()).tray_clicked(now_ms(), visible, anchor);
    match click {
        Click::Hide => {
            let _ = window.hide();
        }
        Click::Ignore => {}
        Click::Open { ticket } => {
            let _ = app.emit("popup-open", ());
            // If the page never answers (for example it failed to load), show the pop-up anyway.
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(SHOW_ANYWAY_AFTER_MS));
                let show = guard(&app.state::<PopupState>()).show_anyway(ticket, now_ms());
                if show {
                    reveal(&app);
                }
            });
        }
    }
}

/// The page has drawn itself and says how big it is (in the page's own units, which are the
/// window's logical pixels). Sizes and places the window, and shows it if it was waiting to open.
pub fn ready(app: &AppHandle, width: f64, height: f64) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    place(app, &window, width.clamp(100.0, 480.0), height.clamp(40.0, 700.0));
    let show = guard(&app.state::<PopupState>()).page_ready(now_ms());
    if show {
        reveal(app);
    }
}

/// Closes the pop-up (Escape, a button, or opening the Hub).
pub fn hide(app: &AppHandle) {
    guard(&app.state::<PopupState>()).closed();
    if let Some(window) = app.get_webview_window(LABEL) {
        // On its own thread, so closing from inside a window event can never block the event loop.
        std::thread::spawn(move || {
            let _ = window.hide();
        });
    }
}

fn reveal(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// The pop-up lost the focus: you clicked somewhere else, so it closes.
fn lost_focus(app: &AppHandle) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let visible = window.is_visible().unwrap_or(false);
    let close = guard(&app.state::<PopupState>()).focus_lost(now_ms(), visible);
    if close {
        std::thread::spawn(move || {
            let _ = window.hide();
        });
    }
}

/// Sizes the window and puts it above the tray icon, kept inside the usable part of that screen
/// (everything is in physical pixels, using that screen's own scale).
fn place(app: &AppHandle, window: &WebviewWindow, width: f64, height: f64) {
    let anchor = guard(&app.state::<PopupState>()).anchor();
    let monitor = app
        .monitor_from_point(anchor.0 as f64, anchor.1 as f64)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let area = Area { x: work.position.x, y: work.position.y, width: work.size.width, height: work.size.height };
    let size = ((width * scale).round() as u32, (height * scale).round() as u32);
    let margin = (placement::SCREEN_MARGIN as f64 * scale).round() as i32;
    let (x, y) = placement::place(anchor, size, area, margin);
    // Moved before and after the resize: on a screen with a different scale Windows may adjust the
    // window in between, and the second move puts it back exactly.
    let _ = window.set_position(PhysicalPosition::new(x, y));
    let _ = window.set_size(PhysicalSize::new(size.0, size.1));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}