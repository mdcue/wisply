//! The "Add idea..." and "Add reminder..." window. One window serves both: the tray menu says which
//! one is wanted, the page (`ui/quick_add.html`) asks for it with `quick_add_kind`, draws the right
//! form and reports its size (`quick_add_ready`). Only then is the window centred on the screen the
//! mouse is on and shown, so you never see a half-drawn form. What the form says, and what is saved,
//! comes from the core (`Hub::quick_add_info`, `companion_add_idea`, `companion_add_reminder`).
//!
//! Unlike the tray pop-up this window does not close when you click elsewhere: you may be looking
//! something up before you write the note.

use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use wisply_core::placement::{self, Area};

pub const LABEL: &str = "quick_add";
const START_WIDTH: f64 = 380.0;
const START_HEIGHT: f64 = 200.0;
/// If the page has not answered after this long, the window is shown anyway.
const SHOW_ANYWAY_AFTER: Duration = Duration::from_millis(900);

/// What the window remembers between the menu click and the page's answer.
struct Inner {
    /// "idea" or "reminder".
    kind: &'static str,
    /// True from the menu click until the window has been shown.
    waiting: bool,
    /// Counts the menu clicks, so an old "show anyway" timer can tell it is out of date.
    ticket: u64,
}

pub struct QuickAddState(Mutex<Inner>);

impl Default for QuickAddState {
    fn default() -> Self {
        QuickAddState(Mutex::new(Inner { kind: "idea", waiting: false, ticket: 0 }))
    }
}

fn lock(state: &QuickAddState) -> MutexGuard<'_, Inner> {
    state.0.lock().unwrap_or_else(|e| e.into_inner())
}

/// Makes the (hidden) window. Call once, when the app starts.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("quick_add.html".into()))
        .title("wisply")
        .inner_size(START_WIDTH, START_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .visible(false)
        .focused(false)
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        // The close button only hides it; it is needed again at the next menu click.
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            hide(&handle);
        }
    });
    Ok(())
}

/// The menu was clicked: `kind` is "idea" or "reminder". Asks the page to draw that form.
pub fn open(app: &AppHandle, kind: &str) {
    if app.get_webview_window(LABEL).is_none() {
        return;
    }
    let ticket = {
        let state = app.state::<QuickAddState>();
        let mut inner = lock(&state);
        inner.kind = if kind == "reminder" { "reminder" } else { "idea" };
        inner.waiting = true;
        inner.ticket += 1;
        inner.ticket
    };
    let _ = app.emit("quick-add-open", ());
    // If the page never answers (for example it failed to load), show the window anyway.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(SHOW_ANYWAY_AFTER);
        let show = {
            let state = app.state::<QuickAddState>();
            let mut inner = lock(&state);
            let show = inner.waiting && inner.ticket == ticket;
            if show {
                inner.waiting = false;
            }
            show
        };
        if show {
            if let Some(window) = app.get_webview_window(LABEL) {
                reveal(&app, &window);
            }
        }
    });
}

/// Which form the page should draw ("idea" or "reminder").
pub fn kind(app: &AppHandle) -> String {
    lock(&app.state::<QuickAddState>()).kind.to_string()
}

/// The page has drawn itself and says how big it is (logical pixels). Sizes the window, and if it
/// was waiting to open, centres it and shows it.
pub fn ready(app: &AppHandle, width: f64, height: f64) {
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let (width, height) = (width.clamp(200.0, 600.0), height.clamp(80.0, 700.0));
    let first_time = {
        let state = app.state::<QuickAddState>();
        let mut inner = lock(&state);
        std::mem::replace(&mut inner.waiting, false)
    };
    if first_time {
        // Put on the right screen first, so the size below is read with that screen's scale.
        move_to_cursor_screen(app, &window);
    }
    let _ = window.set_size(LogicalSize::new(width, height));
    if first_time {
        reveal(app, &window);
    }
}

/// Closes the window (Cancel, Escape, after saving, or opening the Hub).
pub fn hide(app: &AppHandle) {
    lock(&app.state::<QuickAddState>()).waiting = false;
    if let Some(window) = app.get_webview_window(LABEL) {
        // On its own thread, so closing from inside a window event can never block the event loop.
        std::thread::spawn(move || {
            let _ = window.hide();
        });
    }
}

/// The usable part of the screen the mouse is on (else the main screen), and that screen's top-left.
fn cursor_screen_area(app: &AppHandle) -> Option<Area> {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|p| app.monitor_from_point(p.x, p.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let work = monitor.work_area();
    Some(Area { x: work.position.x, y: work.position.y, width: work.size.width, height: work.size.height })
}

fn move_to_cursor_screen(app: &AppHandle, window: &WebviewWindow) {
    if let Some(area) = cursor_screen_area(app) {
        let _ = window.set_position(PhysicalPosition::new(area.x, area.y));
    }
}

/// Centres the window on the screen the mouse is on, shows it and gives it the keyboard.
fn reveal(app: &AppHandle, window: &WebviewWindow) {
    if let (Some(area), Ok(size)) = (cursor_screen_area(app), window.outer_size()) {
        let (x, y) = placement::centered((size.width, size.height), area);
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    let _ = window.show();
    let _ = window.set_focus();
}