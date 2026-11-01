//! The account file (`account.json`): the name given at setup. Same file and same layout as the
//! Python app, so an existing account is picked up as it is.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join("account.json")
}

pub fn exists(dir: &Path) -> bool {
    path(dir).exists()
}

/// The name given at setup, or `None` if there is no account (or it can't be read).
pub fn get_name(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(path(dir)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let name = value.get("name")?.as_str()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Creates the account file. The write is done in two steps (a temporary file, then a rename),
/// so a crash half way through can never leave a broken `account.json` behind.
pub fn create(dir: &Path, name: &str) -> Result<Account> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid("the name can't be empty".to_string()));
    }
    let account = Account {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        created_at: chrono::Local::now().naive_local().format("%Y-%m-%dT%H:%M:%S%.6f").to_string(),
    };
    let target = path(dir);
    let temporary = dir.join("account.json.tmp");
    fs::write(&temporary, serde_json::to_string_pretty(&account)?)?;
    fs::rename(&temporary, &target)?;
    Ok(account)
}