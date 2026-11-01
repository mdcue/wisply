//! The tray icon's moving parts: its hover tip, its red dot, and the pop-ups. Everything that
//! changes what the tray shows goes through here.

use tauri::image::Image;
use tauri::tray::TrayIcon;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use wisply_core::hub::Hub;
use wisply_core::runtime::{self, Update};

/// The tray icon, and the app's icon with and without the red "something is unread" dot.
/// The app keeps one of these in its shared state.
pub struct TrayState {
    pub tray: TrayIcon,
    pub icons: Option<(Image<'static>, Image<'static>)>,
}

/// The app's icon, and the same icon wearing the red dot.
pub fn icons_of(app: &tauri::App) -> Option<(Image<'static>, Image<'static>)> {
    let base = app.default_window_icon()?;
    let (width, height) = (base.width(), base.height());
    let plain = Image::new_owned(base.rgba().to_vec(), width, height);
    let badge = Image::new_owned(wisply_core::icons::badge_from_rgba(base.rgba(), width, height)?, width, height);
    Some((plain, badge))
}

/// Shows the hover tip and the right icon.
pub fn show(app: &AppHandle, update: &Update) {
    let Some(state) = app.try_state::<TrayState>() else { return };
    let _ = state.tray.set_tooltip(Some(update.tooltip.as_str()));
    if let Some((plain, badge)) = &state.icons {
        let _ = state.tray.set_icon(Some(if update.has_unread { badge.clone() } else { plain.clone() }));
    }
}

/// Looks at the data as it is now and shows it (for example after something was read in the Hub).
pub fn refresh(app: &AppHandle) {
    let Some(hub) = app.try_state::<Hub>() else { return };
    match runtime::status_in(hub.data_dir()) {
        Ok(update) => show(app, &update),
        Err(error) => log::warn!("could not refresh the tray: {error}"),
    }
}

/// Tells the Hub page to redraw, and brings the tray up to date.
pub fn changed(app: &AppHandle) {
    let _ = app.emit("data-changed", ());
    refresh(app);
}

/// A pop-up (and a banner in the Hub) for every wisp that found something new.
pub fn announce(app: &AppHandle, update: &Update) {
    for (name, count) in &update.new_from {
        let shown = app
            .notification()
            .builder()
            .title("Wisply")
            .body(wisply_core::wisps::companion_voice::new_items(name))
            .show();
        if let Err(error) = shown {
            log::warn!("could not show a pop-up: {error}");
        }
        let _ = app.emit("new-items", serde_json::json!({ "name": name, "count": count }));
    }
}
