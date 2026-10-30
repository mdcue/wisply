// Hides the extra console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod popup;
mod quick_add;
mod tray;

use std::path::PathBuf;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use wisply_core::hub::Hub;
use wisply_core::{autostart, runtime};

const HUB_LABEL: &str = "hub";

/// Shows the Hub, creating the window the first time it is needed. (Without a data folder there is
/// nothing to show, so nothing opens.)
pub(crate) fn open_hub(app: &AppHandle) {
    if app.try_state::<Hub>().is_none() {
        return;
    }
    if let Some(window) = app.get_webview_window(HUB_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, HUB_LABEL, WebviewUrl::App("dashboard.html".into()))
        .title("wisply Hub")
        .inner_size(420.0, 600.0)
        .center()
        .build();
    if let Err(error) = built {
        eprintln!("could not open the Hub: {error}");
    }
}

/// Opens the "Add idea" or "Add reminder" window. Before setup the Hub is where you introduce
/// yourself, and without a data folder there is nothing to add to.
fn open_quick_add(app: &AppHandle, kind: &str) {
    match app.try_state::<Hub>() {
        None => {}
        Some(hub) if !hub.account_exists() => open_hub(app),
        Some(_) => quick_add::open(app, kind),
    }
}

/// Gets the data ready: the log file, the database (importing the old app's data on the very first
/// start) and the three wisps. Gives back the data folder (so the checks can start) and the text
/// for the tray icon's hover tip. If anything is wrong, there is no folder and the tip says why.
fn startup() -> (Option<PathBuf>, String) {
    let dir = match wisply_core::paths::data_dir() {
        Ok(dir) => dir,
        Err(error) => return (None, format!("Wisply: {error}")),
    };
    if let Err(error) = wisply_core::logging::init(&dir) {
        eprintln!("could not start the log file: {error}");
    }
    match runtime::prepare(&dir) {
        Ok(Some(r)) => {
            let tip = format!(
                "Wisply: imported {} wisps, {} items, {} notes from the old app",
                r.wisps, r.items, r.notes
            );
            (Some(dir), tip)
        }
        Ok(None) => (Some(dir), "Wisply".to_string()),
        Err(error) => (None, format!("Wisply: {error}")),
    }
}

fn main() {
    let app = tauri::Builder::default()
        // Must come first. A second copy of Wisply doesn't start: it asks this one to show the Hub
        // (unless Windows started it in the background) and quits.
        .plugin(tauri_plugin_single_instance::init(|app, args, _directory| {
            if !args.iter().any(|a| a == autostart::BACKGROUND_FLAG) {
                open_hub(app);
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            bridge::account_exists,
            bridge::create_account,
            bridge::get_expressions,
            bridge::get_wisps,
            bridge::get_settings_wisps,
            bridge::rename_wisp,
            bridge::recolor_wisp,
            bridge::set_expression,
            bridge::set_fetch_interval,
            bridge::toggle_active,
            bridge::fetch_now,
            bridge::get_channels,
            bridge::get_activity_feed,
            bridge::mark_item_read,
            bridge::mark_all_read,
            bridge::dismiss_item,
            bridge::clear_read,
            bridge::open_item,
            bridge::companion_notes,
            bridge::companion_mood,
            bridge::companion_add_reminder,
            bridge::companion_add_idea,
            bridge::companion_update,
            bridge::companion_complete,
            bridge::companion_snooze,
            bridge::companion_delete,
            bridge::open_data_folder,
            bridge::quit_app,
            bridge::popup_ready,
            bridge::popup_hide,
            bridge::open_hub_window,
            bridge::quick_add_info,
            bridge::quick_add_kind,
            bridge::quick_add_ready,
            bridge::quick_add_hide,
        ])
        .setup(|app| {
            let (data_dir, tooltip) = startup();
            if let Some(dir) = &data_dir {
                app.manage(Hub::new(dir));
            }
            // The tray pop-up: made now (hidden), so a click on the tray opens it at once.
            app.manage(popup::PopupState::default());
            if data_dir.is_some() {
                if let Err(error) = popup::create(app.handle()) {
                    log::warn!("could not make the tray pop-up: {error}");
                }
            }
            // The "Add idea / Add reminder" window: also made now, hidden.
            app.manage(quick_add::QuickAddState::default());
            if data_dir.is_some() {
                if let Err(error) = quick_add::create(app.handle()) {
                    log::warn!("could not make the quick-add window: {error}");
                }
            }

            let open = MenuItem::with_id(app, "open", "Open Hub", true, None::<&str>)?;
            let add_idea = MenuItem::with_id(app, "add_idea", "Add idea\u{2026}", true, None::<&str>)?;
            let add_reminder = MenuItem::with_id(app, "add_reminder", "Add reminder\u{2026}", true, None::<&str>)?;
            let start_with_windows =
                CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, autostart::is_enabled(), None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &open,
                    &add_idea,
                    &add_reminder,
                    &PredefinedMenuItem::separator(app)?,
                    &start_with_windows,
                    &quit,
                ],
            )?;

            let icons = tray::icons_of(app);
            let unread_at_start = data_dir.as_deref().and_then(|dir| runtime::status_in(dir).ok());
            let wearing_badge = unread_at_start.as_ref().is_some_and(|u| u.has_unread);

            // The first start after the old app's data was imported says so; otherwise show the count.
            let first_tip = if tooltip == "Wisply" {
                unread_at_start.map(|u| u.tooltip).unwrap_or(tooltip)
            } else {
                tooltip
            };

            let mut builder = TrayIconBuilder::new()
                .tooltip(first_tip)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event({
                    let start_with_windows = start_with_windows.clone();
                    move |app, event| match event.id().as_ref() {
                        "open" => open_hub(app),
                        "add_idea" => open_quick_add(app, "idea"),
                        "add_reminder" => open_quick_add(app, "reminder"),
                        "autostart" => {
                            // Windows flips the tick by itself; afterwards show what is really set.
                            if let Err(error) = autostart::toggle() {
                                log::warn!("could not change Start with Windows: {error}");
                            }
                            let _ = start_with_windows.set_checked(autostart::is_enabled());
                        }
                        "quit" => app.exit(0),
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        position,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        match app.try_state::<Hub>() {
                            // Without a data folder there is nothing to show.
                            None => {}
                            // Before setup the Hub is where you introduce yourself.
                            Some(hub) if !hub.account_exists() => open_hub(app),
                            Some(_) => popup::toggle(app, popup::anchor_of(&rect, position)),
                        }
                    }
                });
            if let Some((plain, badge)) = &icons {
                builder = builder.icon(if wearing_badge { badge.clone() } else { plain.clone() });
            }
            app.manage(tray::TrayState { tray: builder.build(app)?, icons });

            if let Some(dir) = data_dir {
                // Draw any missing or out-of-date wisp icons, off to the side so start-up isn't held up.
                let for_icons = dir.clone();
                std::thread::spawn(move || runtime::refresh_icons(&for_icons));

                // Start checking the wisps in the background. After each round the tray shows the
                // unread count and the red dot, the Hub (if open) redraws, and every wisp that found
                // something gets a pop-up. The handle must stay alive for as long as the app runs
                // (dropping it would stop the checks), so the app keeps it.
                let app_handle = app.handle().clone();
                let handle = runtime::start(&dir, move |update| {
                    tray::show(&app_handle, &update);
                    tray::announce(&app_handle, &update);
                    let _ = app_handle.emit("data-changed", ());
                });
                app.manage(handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Wisply");

    app.run(|_app, event| {
        // Closing the Hub must not quit the app: it lives in the tray. Only "Quit" (code Some) exits.
        if let RunEvent::ExitRequested { code, api, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}