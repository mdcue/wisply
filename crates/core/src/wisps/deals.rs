//! Game Deals ("ware-Weaver"): tracks game deals and free games from CheapShark.

use std::collections::HashMap;

use rusqlite::Connection;
use serde_json::Value;
use url::Url;

use super::{register, ChannelDef, WispDef};
use crate::http::Client;
use crate::safe::{clean_text, MAX_ITEM_TEXT};
use crate::scheduler::Fetcher;
use crate::store::{self, NewItem};

pub const DEF: WispDef = WispDef {
    id: "game_deals",
    name: "ware-Weaver",
    color: "#4CAF50",
    description: "Tracks game deals and free games across every major store.",
    channels: &[
        ChannelDef { id: "free_games", title: "Free games", description: "Games that are free right now", color: "#5df300" },
        ChannelDef { id: "deals", title: "Deals", description: "Games on sale", color: "#2196F3" },
    ],
    expression: "smiling",
    fetch_interval: None,
};

const API: &str = "https://www.cheapshark.com/api/1.0";
const HOST: &str = "www.cheapshark.com";
const REDIRECT: &str = "https://www.cheapshark.com/redirect";
/// How many of the best deals are looked at each time.
const TOP_N: usize = 10;
const UPPER_PRICE_FOR_DEALS: &str = "60";

pub fn register_wisp(conn: &Connection) -> crate::Result<()> {
    register(conn, &DEF)
}

pub struct DealsFetcher {
    client: Client,
    base: String,
}

impl DealsFetcher {
    pub fn new() -> DealsFetcher {
        DealsFetcher { client: Client::strict(&[HOST]), base: API.to_string() }
    }

    /// For tests: a client and a base address of your own (a fake website on this computer).
    pub fn with_client(client: Client, base: &str) -> DealsFetcher {
        DealsFetcher { client, base: base.trim_end_matches('/').to_string() }
    }

    /// Fetches one address and returns its JSON.
    fn get_json(&self, path: &str, query: &[(&str, &str)], what: &str) -> Result<Value, String> {
        let url = Url::parse_with_params(&format!("{}/{}", self.base, path), query)
            .map_err(|e| format!("bad address: {e}"))?;
        let reply = self.client.get(&url, &[])?;
        if !reply.is_success() {
            return Err(format!("CheapShark ({what}) answered with status {}", reply.status));
        }
        serde_json::from_str(&reply.body).map_err(|_| format!("CheapShark ({what}) sent something that isn't JSON"))
    }
}

impl Default for DealsFetcher {
    fn default() -> Self {
        DealsFetcher::new()
    }
}

impl Fetcher for DealsFetcher {
    fn wisp_id(&self) -> &str {
        DEF.id
    }

    fn fetch(&mut self, conn: &Connection) -> Result<(), String> {
        let stores = store_names(&self.get_json("stores", &[], "stores")?)?;
        let deals = self.get_json("deals", &[("upperPrice", UPPER_PRICE_FOR_DEALS)], "deals")?;
        save_deals(conn, &deals, &stores)?;
        let free = self.get_json("deals", &[("upperPrice", "0")], "free games")?;
        save_free_games(conn, &free, &stores)?;
        Ok(())
    }
}

/// A JSON string or number as text.
fn text_of(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn number_of(value: Option<&Value>) -> Option<f64> {
    let number = text_of(value)?.parse::<f64>().ok()?;
    number.is_finite().then_some(number)
}

fn is_on(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|n| n != 0.0),
        Some(Value::String(s)) => !s.is_empty() && s != "0",
        _ => false,
    }
}

/// Store id -> store name, for the stores that are switched on.
pub fn store_names(stores: &Value) -> Result<HashMap<String, String>, String> {
    let list = stores.as_array().ok_or("CheapShark's store list was not a list")?;
    Ok(list
        .iter()
        .filter(|s| is_on(s.get("isActive")))
        .filter_map(|s| Some((text_of(s.get("storeID"))?, clean_text(&text_of(s.get("storeName"))?, 60))))
        .collect())
}

/// True if a deal id can be put into the link exactly as it is: letters, digits, `- _ . ~`, and
/// `%XX` escapes. CheapShark's ids arrive already percent-encoded (like `...%2B...%3D`), so they
/// must NOT be encoded a second time; anything else could add to the address, so it is refused.
fn is_ready_to_use_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => i += 1,
            b'%' if bytes.get(i + 1..i + 3).is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit)) => i += 3,
            _ => return false,
        }
    }
    !bytes.is_empty()
}

/// The address that opens a deal in the browser, or `None` if the id looks wrong (the item is then
/// stored without a link).
fn deal_link(deal_id: &str) -> Option<String> {
    if !is_ready_to_use_id(deal_id) {
        return None;
    }
    Url::parse(&format!("{REDIRECT}?dealID={deal_id}")).ok().map(|u| u.to_string())
}

fn store_of(deal: &Value, stores: &HashMap<String, String>) -> String {
    text_of(deal.get("storeID")).and_then(|id| stores.get(&id).cloned()).unwrap_or_else(|| "Unknown store".to_string())
}

/// Adds the deals on sale. The best deal is added last, so the Hub (newest first) lists it first.
pub fn save_deals(conn: &Connection, deals: &Value, stores: &HashMap<String, String>) -> Result<usize, String> {
    let list = deals.as_array().ok_or("CheapShark's deal list was not a list")?;
    let mut added = 0;
    for deal in list.iter().take(TOP_N).rev() {
        let (Some(deal_id), Some(title)) = (text_of(deal.get("dealID")), text_of(deal.get("title"))) else { continue };
        let Some(price_text) = text_of(deal.get("salePrice")) else { continue };
        let Some(price) = number_of(deal.get("salePrice")) else { continue };
        if price <= 0.0 {
            continue;
        }
        let normal = text_of(deal.get("normalPrice")).unwrap_or_else(|| "?".to_string());
        let savings = number_of(deal.get("savings")).unwrap_or(0.0).round_ties_even().clamp(0.0, 100.0) as i64;
        let text = format!(
            "{} - ${} (was ${}, {}% off) ({})",
            clean_text(&title, 120),
            clean_text(&price_text, 12),
            clean_text(&normal, 12),
            savings,
            store_of(deal, stores)
        );
        let item = NewItem {
            channel_id: "deals".into(),
            dedupe_key: deal_id.clone(),
            text: clean_text(&text, MAX_ITEM_TEXT),
            url: deal_link(&deal_id),
            price: Some(price),
            discount_percent: Some(savings),
            ..Default::default()
        };
        if store::add_item(conn, &item).map_err(|e| e.to_string())? {
            added += 1;
        }
    }
    Ok(added)
}

/// Adds the games that are free right now (same order rule as `save_deals`).
pub fn save_free_games(conn: &Connection, deals: &Value, stores: &HashMap<String, String>) -> Result<usize, String> {
    let list = deals.as_array().ok_or("CheapShark's free game list was not a list")?;
    let mut added = 0;
    for deal in list.iter().take(TOP_N).rev() {
        let (Some(deal_id), Some(title)) = (text_of(deal.get("dealID")), text_of(deal.get("title"))) else { continue };
        let normal = text_of(deal.get("normalPrice")).unwrap_or_else(|| "?".to_string());
        let text = format!(
            "{} is free right now! (normally ${}) ({})",
            clean_text(&title, 120),
            clean_text(&normal, 12),
            store_of(deal, stores)
        );
        let item = NewItem {
            channel_id: "free_games".into(),
            dedupe_key: deal_id.clone(),
            text: clean_text(&text, MAX_ITEM_TEXT),
            url: deal_link(&deal_id),
            price: Some(0.0),
            discount_percent: Some(100),
            ..Default::default()
        };
        if store::add_item(conn, &item).map_err(|e| e.to_string())? {
            added += 1;
        }
    }
    Ok(added)
}