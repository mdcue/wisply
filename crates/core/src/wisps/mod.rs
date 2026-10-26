//! The wisps themselves. Each one has a fixed description of itself (`WispDef`) and a `Fetcher`
//! that checks its source. The description is what the Python app called `register_wisp`.

pub mod deals;

use rusqlite::Connection;

use crate::store::{self, WispChanges};
use crate::Result;

pub struct ChannelDef {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub color: &'static str,
}

pub struct WispDef {
    pub id: &'static str,
    pub name: &'static str,
    pub color: &'static str,
    pub description: &'static str,
    /// Shown in this order.
    pub channels: &'static [ChannelDef],
    pub expression: &'static str,
    /// Only used the first time the wisp is added; `None` keeps the default (one hour).
    pub fetch_interval: Option<i64>,
}

/// Adds a wisp and its channels. A wisp's look and interval are only set the first time it is
/// added, so changes the user makes are kept. Safe to call on every start.
pub fn register(conn: &Connection, def: &WispDef) -> Result<()> {
    let is_new = store::get_wisp(conn, def.id)?.is_none();
    store::add_wisp(conn, def.id, def.name, def.color)?;
    if is_new {
        let looks = WispChanges { expression: Some(def.expression.to_string()), ..Default::default() };
        store::update_wisp(conn, def.id, &looks)?;
        if let Some(seconds) = def.fetch_interval {
            store::set_fetch_interval(conn, def.id, seconds)?;
        }
    }
    for (position, channel) in def.channels.iter().enumerate() {
        store::upsert_channel(conn, channel.id, def.id, channel.title, channel.description, channel.color, position as i64)?;
    }
    Ok(())
}