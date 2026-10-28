// Hides the extra console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;
use wisply_core::{autostart, runtime};

const HUB_LABEL: &str = "hub";

/// Shows the Hub, creating the window the first time it is needed.
fn open_hub(app: &AppHandle) {
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

/// The app's icon, and the same icon with the red "something is unread" dot.
fn tray_icons(app: &tauri::App) -> Option<(Image<'static>, Image<'static>)> {
    let base = app.default_window_icon()?;
    let (width, height) = (base.width(), base.height());
    let plain = Image::new_owned(base.rgba().to_vec(), width, height);
    let badge = Image::new_owned(wisply_core::icons::badge_from_rgba(base.rgba(), width, height)?, width, height);
    Some((plain, badge))
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
        .setup(|app| {
            let (data_dir, tooltip) = startup();

            let open = MenuItem::with_id(app, "open", "Open Hub", true, None::<&str>)?;
            let start_with_windows =
                CheckMenuItem::with_id(app, "autostart", "Start with Windows", true, autostart::is_enabled(), None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&open, &PredefinedMenuItem::separator(app)?, &start_with_windows, &quit],
            )?;

            let icons = tray_icons(app);
            let unread_at_start = data_dir.as_deref().and_then(|dir| runtime::status_in(dir).ok());
            let wearing_badge = unread_at_start.as_ref().is_some_and(|u| u.has_unread);

            // The first start after the old app's data was imported says so; otherwise show the count.
            let first_tip = if tooltip == "Wisply" {
                unread_at_start.map(|u| u.tooltip).unwrap_or(tooltip)
            } else {
                tooltip
            };

            let mut tray = TrayIconBuilder::new()
                .tooltip(first_tip)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event({
                    let start_with_windows = start_with_windows.clone();
                    move |app, event| match event.id().as_ref() {
                        "open" => open_hub(app),
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
                        ..
                    } = event
                    {
                        open_hub(tray.app_handle());
                    }
                });
            if let Some((plain, badge)) = &icons {
                tray = tray.icon(if wearing_badge { badge.clone() } else { plain.clone() });
            }
            let tray = tray.build(app)?;

            if let Some(dir) = data_dir {
                // Draw any missing or out-of-date wisp icons, off to the side so start-up isn't held up.
                let for_icons = dir.clone();
                std::thread::spawn(move || runtime::refresh_icons(&for_icons));

                // Start checking the wisps in the background. After each round the tray shows the
                // unread count and the red dot, and every wisp that found something gets a pop-up.
                // The handle must stay alive for as long as the app runs (dropping it would stop the
                // checks), so the app keeps it.
                let app_handle = app.handle().clone();
                let handle = runtime::start(&dir, move |update| {
                    let _ = tray.set_tooltip(Some(update.tooltip));
                    if let Some((plain, badge)) = &icons {
                        let _ = tray.set_icon(Some(if update.has_unread { badge.clone() } else { plain.clone() }));
                    }
                    for name in update.new_from {
                        let shown = app_handle
                            .notification()
                            .builder()
                            .title("Wisply")
                            .body(format!("{name} has something new for you"))
                            .show();
                        if let Err(error) = shown {
                            log::warn!("could not show a pop-up: {error}");
                        }
                    }
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
