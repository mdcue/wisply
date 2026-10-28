//! "Start with Windows": a value in the current user's `Run` list in the registry that starts
//! Wisply when they sign in. It only touches that one value, and only the current user's list
//! (no administrator rights needed). On systems other than Windows it does nothing.

use std::path::Path;

/// The name of the value (the same one the Python app used, so turning it on replaces the old
/// entry rather than starting two programs).
pub const NAME: &str = "wisply";

/// The extra word that tells Wisply it was started by Windows and should stay quiet in the tray.
pub const BACKGROUND_FLAG: &str = "--background";

/// The command Windows runs at sign-in: the program, in quotes, then the background flag.
pub fn command_for(exe: &Path) -> String {
    format!("\"{}\" {BACKGROUND_FLAG}", exe.display())
}

/// Whether Wisply starts with Windows.
pub fn is_enabled() -> bool {
    imp::is_enabled(NAME)
}

/// Makes Wisply start with Windows, using the program that is running now.
pub fn enable() -> std::io::Result<()> {
    imp::enable(NAME, &command_for(&std::env::current_exe()?))
}

/// Stops Wisply starting with Windows. Fine if it wasn't set.
pub fn disable() -> std::io::Result<()> {
    imp::disable(NAME)
}

/// Switches it on if it is off and off if it is on. Gives back the new state.
pub fn toggle() -> std::io::Result<bool> {
    if is_enabled() {
        disable()?;
    } else {
        enable()?;
    }
    Ok(is_enabled())
}

/// The registry part, with the value name as a parameter so tests can use their own name.
#[cfg(windows)]
pub mod imp {
    use std::io;

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
    use winreg::RegKey;

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    pub fn is_enabled(name: &str) -> bool {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(RUN_KEY, KEY_READ)
            .and_then(|key| key.get_value::<String, _>(name))
            .is_ok()
    }

    pub fn enable(name: &str, command: &str) -> io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN_KEY)?;
        key.set_value(name, &command)
    }

    pub fn disable(name: &str) -> io::Result<()> {
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)?;
        match key.delete_value(name) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }

    /// The stored command, if the value exists.
    pub fn command(name: &str) -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_READ).ok()?.get_value(name).ok()
    }
}

#[cfg(not(windows))]
pub mod imp {
    use std::io;

    pub fn is_enabled(_name: &str) -> bool {
        false
    }

    pub fn enable(_name: &str, _command: &str) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Start with Windows only works on Windows"))
    }

    pub fn disable(_name: &str) -> io::Result<()> {
        Ok(())
    }

    pub fn command(_name: &str) -> Option<String> {
        None
    }
}
