// Hides the extra console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

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
    match wisply_core::runtime::prepare(&dir) {
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
        .setup(|app| {
            let (data_dir, tooltip) = startup();

            let open = MenuItem::with_id(app, "open", "Open Hub", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&open, &separator, &quit])?;

            let mut tray = TrayIconBuilder::new()
                .tooltip(tooltip)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => open_hub(app),
                    "quit" => app.exit(0),
                    _ => {}
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
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            let tray = tray.build(app)?;

            // Start checking the wisps in the background. Each time a round of checks finishes, the
            // hover tip shows how many unread items there are. The handle must stay alive for as
            // long as the app runs (dropping it would stop the checks), so the app keeps it.
            if let Some(dir) = data_dir {
                let handle = wisply_core::runtime::start(&dir, move |text| {
                    let _ = tray.set_tooltip(Some(text));
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