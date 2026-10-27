//! Where wisply will keep its files. Same rules as the Python app: `%APPDATA%\Wisply` by default,
//! or a folder named in `WISPLY_DATA_DIR` (handy for tests with a clean folder).

use std::env;
use std::fs;
use std::path::PathBuf;

use crate::Result;

///The writable data folder. It is created if it doesn't exist yet.
pub fn data_dir() -> Result<PathBuf> {
    let dir = match env::var_os("WISPLY_DATA_DIR") {
        Some(custom) if !custom.is_empty() => PathBuf::from(custom),
        _ => default_data_dir(),
    };
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn default_data_dir() -> PathBuf {
    if let Some(appdata) = env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("Wisply");
    }
    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".wisply")
}

/// File name of the new app's database.
pub const DB_FILE: &str = "wisply.db";

/// The new app's database. The old Python app's `data.db` is never written to.
pub fn db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join(DB_FILE))
}

/// The old Python app's database, read only by the importer.
pub fn legacy_db_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("data.db"))
}