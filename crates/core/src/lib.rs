//! Wisply's core: everything that is not a window.
//! Keeping it separate from the app means it can be built and tested without any user interface.
pub mod account;
pub mod http;
pub mod legacy;
pub mod logging;
pub mod paths;
pub mod safe;
pub mod scheduler;
pub mod schema;
pub mod store;
pub mod wisps;

use std::path::PathBuf;

//Everything that can go wrong in the core. The messages are written to be shown to a person.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database problem: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("file problem: {0}")]
    Io(#[from] std::io::Error),

    #[error("this data was made by a newer Wisply (data format {found}, this version understands up to {supported})")]
    NewerDatabase { found: i32, supported: i32},

    #[error("the new database already has wisps in it, so nothing was imported")]
    TargetNotEmpty,

    #[error("the old database was not found at {0}")]
    LegacyMissing(PathBuf),

    #[error("the old database doesn't look like a Wisply database: {0}")]
    LegacyInvalid(String),

    #[error("settings file problem: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, Error>;

//What happened when the app opened its data
#[derive(Debug)]
pub struct Opened {
    pub conn: rusqlite::Connection,
    ///Set only on the very first start of the new app, when the old Python data was found and copied.
    pub imported: Option<legacy::Report>,
}

///Opens `wisply.db` inside `dir` (creating and upgrading it).
///`data.db` sits besides it, the old data is imported once. The old file is never changed.
pub fn open_in_dir(dir: &std::path::Path) -> Result<Opened> {
    let db = dir.join("wisply.db");
    let old = dir.join("data.db");
    let first_start = !db.exists();
    let conn = schema::open(&db)?;
    let imported = if first_start && old.exists() { Some(legacy::import(&old, &conn)?) } else { None };
    Ok(Opened { conn, imported })    
}