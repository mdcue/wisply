//! What the Hub page can ask for. Every command here only passes the request to `Hub` (in the core,
//! where it is tested), then tells the page and the tray that something changed. They are all
//! `async` commands so a slow one never freezes the window.

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;
use wisply_core::hub::{
    ChangeResult, ChannelView, FeedItem, Hub, NotesView, SettingsWisp, WispView,
};
use wisply_core::wisps::companion::Feeling;

use crate::{popup, tray};

/// Turns the core's error into text for the page, and keeps the details in the log.
fn logged<T>(what: &str, result: wisply_core::Result<T>) -> Result<T, String> {
    result.map_err(|error| {
        log::warn!("{what} failed: {error}");
        error.to_string()
    })
}

/// Checks a wisp on its own thread (internet wisps can take a while), then tells the page how it
/// went and refreshes everything.
fn check_in_background(app: &AppHandle, wisp_id: &str) {
    let app = app.clone();
    let wisp_id = wisp_id.to_string();
    std::thread::spawn(move || {
        let Some(hub) = app.try_state::<Hub>() else { return };
        match hub.fetch_now(&wisp_id) {
            Ok(Some(report)) => {
                let _ = app.emit(
                    "fetch-finished",
                    serde_json::json!({ "wispId": report.wisp_id, "ok": report.ok, "newItems": report.new_items }),
                );
            }
            Ok(None) => {}
            Err(error) => log::warn!("check for {wisp_id} failed: {error}"),
        }
        tray::changed(&app);
    });
}

/// After a change to Companion's notes: refresh, and let pip react (its mood and face).
fn companion_changed(app: &AppHandle, result: ChangeResult) -> ChangeResult {
    tray::changed(app);
    if result.ok {
        check_in_background(app, "companion");
    }
    result
}

//account
#[tauri::command(async)]
pub fn account_exists(hub: State<'_, Hub>) -> bool {
    hub.account_exists()
}

#[tauri::command(async)]
pub fn create_account(hub: State<'_, Hub>, name: String) -> bool {
    hub.create_account(&name)
}

//wisps
#[tauri::command(async)]
pub fn get_expressions(hub: State<'_, Hub>) -> Vec<&'static str> {
    hub.expressions()
}

#[tauri::command(async)]
pub fn get_wisps(hub: State<'_, Hub>) -> Result<Vec<WispView>, String> {
    logged("get_wisps", hub.wisps())
}

#[tauri::command(async)]
pub fn get_settings_wisps(hub: State<'_, Hub>) -> Result<Vec<SettingsWisp>, String> {
    logged("get_settings_wisps", hub.settings_wisps())
}

#[tauri::command(async)]
pub fn rename_wisp(app: AppHandle, hub: State<'_, Hub>, wisp_id: String, new_name: String) -> Result<bool, String> {
    let done = logged("rename_wisp", hub.rename_wisp(&wisp_id, &new_name))?;
    tray::changed(&app);
    Ok(done)
}

#[tauri::command(async)]
pub fn recolor_wisp(app: AppHandle, hub: State<'_, Hub>, wisp_id: String, color: String) -> Result<bool, String> {
    let done = logged("recolor_wisp", hub.recolor_wisp(&wisp_id, &color))?;
    tray::changed(&app);
    Ok(done)
}

#[tauri::command(async)]
pub fn set_expression(app: AppHandle, hub: State<'_, Hub>, wisp_id: String, expression: String) -> Result<bool, String> {
    let done = logged("set_expression", hub.set_expression(&wisp_id, &expression))?;
    tray::changed(&app);
    Ok(done)
}

#[tauri::command(async)]
pub fn set_fetch_interval(hub: State<'_, Hub>, wisp_id: String, seconds: i64) -> Result<(), String> {
    logged("set_fetch_interval", hub.set_fetch_interval(&wisp_id, seconds))
}

/// Switches a wisp on or off; switching on also checks it straight away.
#[tauri::command(async)]
pub fn toggle_active(app: AppHandle, hub: State<'_, Hub>, wisp_id: String) -> Result<bool, String> {
    let now_on = logged("toggle_active", hub.toggle_active(&wisp_id))?.unwrap_or(false);
    tray::changed(&app);
    if now_on {
        check_in_background(&app, &wisp_id);
    }
    Ok(now_on)
}

/// Starts a check and returns at once; the page hears how it went from the "fetch-finished" event.
#[tauri::command(async)]
pub fn fetch_now(app: AppHandle, wisp_id: String) {
    check_in_background(&app, &wisp_id);
}

//items
#[tauri::command(async)]
pub fn get_channels(hub: State<'_, Hub>, wisp_id: String) -> Result<Vec<ChannelView>, String> {
    logged("get_channels", hub.channels(&wisp_id))
}

#[tauri::command(async)]
pub fn get_activity_feed(hub: State<'_, Hub>) -> Result<Vec<FeedItem>, String> {
    logged("get_activity_feed", hub.activity_feed())
}

#[tauri::command(async)]
pub fn mark_item_read(app: AppHandle, hub: State<'_, Hub>, item_id: i64) -> Result<(), String> {
    logged("mark_item_read", hub.mark_item_read(item_id))?;
    tray::changed(&app);
    Ok(())
}

#[tauri::command(async)]
pub fn mark_all_read(app: AppHandle, hub: State<'_, Hub>, channel_id: String) -> Result<(), String> {
    logged("mark_all_read", hub.mark_all_read(&channel_id))?;
    tray::changed(&app);
    Ok(())
}

#[tauri::command(async)]
pub fn dismiss_item(app: AppHandle, hub: State<'_, Hub>, item_id: i64) -> Result<(), String> {
    logged("dismiss_item", hub.dismiss_item(item_id))?;
    tray::changed(&app);
    Ok(())
}

#[tauri::command(async)]
pub fn clear_read(app: AppHandle, hub: State<'_, Hub>, channel_id: String) -> Result<(), String> {
    logged("clear_read", hub.clear_read(&channel_id))?;
    tray::changed(&app);
    Ok(())
}

/// Marks the item read and opens its link in the default browser. The link is looked up in the
/// database (the page can't choose where it goes) and only `https` links are opened.
#[tauri::command(async)]
pub fn open_item(app: AppHandle, hub: State<'_, Hub>, item_id: i64) -> Result<(), String> {
    let link = logged("open_item", hub.open_item(item_id))?;
    tray::changed(&app);
    if let Some(link) = link {
        if let Err(error) = app.opener().open_url(link, None::<&str>) {
            log::warn!("could not open a link: {error}");
        }
    }
    Ok(())
}

//companion
#[tauri::command(async)]
pub fn companion_notes(hub: State<'_, Hub>) -> Result<NotesView, String> {
    logged("companion_notes", hub.companion_notes())
}

#[tauri::command(async)]
pub fn companion_mood(hub: State<'_, Hub>) -> Result<Feeling, String> {
    logged("companion_mood", hub.companion_mood())
}

#[tauri::command(async)]
pub fn companion_add_reminder(
    app: AppHandle,
    hub: State<'_, Hub>,
    text: String,
    date_text: String,
    time_text: String,
    repeat: String,
) -> ChangeResult {
    companion_changed(&app, hub.companion_add_reminder(&text, &date_text, &time_text, &repeat))
}

#[tauri::command(async)]
pub fn companion_add_idea(app: AppHandle, hub: State<'_, Hub>, text: String) -> ChangeResult {
    companion_changed(&app, hub.companion_add_idea(&text))
}

#[tauri::command(async)]
pub fn companion_update(
    app: AppHandle,
    hub: State<'_, Hub>,
    note_id: i64,
    text: String,
    date_text: String,
    time_text: String,
    repeat: String,
) -> ChangeResult {
    companion_changed(&app, hub.companion_update(note_id, &text, &date_text, &time_text, &repeat))
}

#[tauri::command(async)]
pub fn companion_complete(app: AppHandle, hub: State<'_, Hub>, note_id: i64) -> ChangeResult {
    companion_changed(&app, hub.companion_complete(note_id))
}

#[tauri::command(async)]
pub fn companion_snooze(app: AppHandle, hub: State<'_, Hub>, note_id: i64, option: String) -> ChangeResult {
    companion_changed(&app, hub.companion_snooze(note_id, &option))
}

#[tauri::command(async)]
pub fn companion_delete(app: AppHandle, hub: State<'_, Hub>, note_id: i64) -> ChangeResult {
    companion_changed(&app, hub.companion_delete(note_id))
}

//the app
#[tauri::command(async)]
pub fn open_data_folder(app: AppHandle, hub: State<'_, Hub>) {
    let folder = hub.data_dir().to_string_lossy().to_string();
    if let Err(error) = app.opener().open_path(folder, None::<&str>) {
        log::warn!("could not open the data folder: {error}");
    }
}

#[tauri::command(async)]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

//the tray pop-up
/// The pop-up page has drawn itself and says how big it is. This is what shows the pop-up.
#[tauri::command(async)]
pub fn popup_ready(app: AppHandle, width: f64, height: f64) {
    popup::ready(&app, width, height);
}

#[tauri::command(async)]
pub fn popup_hide(app: AppHandle) {
    popup::hide(&app);
}

/// The "Open Hub" button in the pop-up.
#[tauri::command(async)]
pub fn open_hub_window(app: AppHandle) {
    popup::hide(&app);
    crate::open_hub(&app);
}