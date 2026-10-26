// Hides the extra console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

/// A one-line summary of the startup, shown when hovering the tray icon.
fn startup_summary() -> String {
    let dir = match wisply_core::paths::data_dir() {
        Ok(dir) => dir,
        Err(error) => return format!("Wisply: {error}"),
    };
    match wisply_core::open_in_dir(&dir) {
        Ok(opened) => match opened.imported {
            Some(r) => format!(
                "Wisply: imported {} wisps, {} items, {} notes from the old app",
                r.wisps, r.items, r.notes
            ),
            None => "Wisply".to_string(),
        },
        Err(error) => format!("Wisply: {error}"),
    }
}

fn main() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let tooltip = startup_summary();

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
            tray.build(app)?;
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