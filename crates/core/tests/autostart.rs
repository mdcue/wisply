//! Tests for "Start with Windows". The command is checked everywhere; the registry itself only on
//! Windows, using a value name of its own so a real "wisply" entry is never touched.

use std::path::Path;

use wisply_core::autostart;

#[test]
fn the_command_quotes_the_program_and_asks_to_stay_in_the_background() {
    assert_eq!(
        autostart::command_for(Path::new(r"C:\Program Files\Wisply\wisply.exe")),
        r#""C:\Program Files\Wisply\wisply.exe" --background"#
    );
}

#[cfg(windows)]
#[test]
fn the_registry_entry_can_be_added_read_and_removed() {
    let name = format!("wisply-test-{}", uuid::Uuid::new_v4());
    assert!(!autostart::imp::is_enabled(&name));
    autostart::imp::enable(&name, r#""C:\x\wisply.exe" --background"#).unwrap();
    assert!(autostart::imp::is_enabled(&name));
    assert_eq!(autostart::imp::command(&name).as_deref(), Some(r#""C:\x\wisply.exe" --background"#));
    autostart::imp::disable(&name).unwrap();
    assert!(!autostart::imp::is_enabled(&name));
    autostart::imp::disable(&name).unwrap(); // removing what is not there is fine
}

#[cfg(not(windows))]
#[test]
fn elsewhere_it_says_no_and_refuses_politely() {
    assert!(!autostart::is_enabled());
    assert!(autostart::enable().is_err());
    assert!(autostart::disable().is_ok());
}
