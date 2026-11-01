//! Developer tool: runs the real Game Deals check against the real CheapShark and prints what it
//! found. It uses a throw-away database in memory, so it changes nothing on disk.
//!
//!     cargo run -p wisply-core --example try_deals

use wisply_core::scheduler::Fetcher;
use wisply_core::wisps::deals::{self, DealsFetcher};
use wisply_core::{schema, store};

fn main() {
    let conn = schema::open_in_memory().expect("could not open a database");
    deals::register_wisp(&conn).expect("could not register the wisp");

    println!("Asking CheapShark...");
    if let Err(error) = DealsFetcher::new().fetch(&conn) {
        eprintln!("The check failed: {error}");
        std::process::exit(1);
    }
    for channel in ["deals", "free_games"] {
        let items = store::items(&conn, channel, None).expect("could not read items");
        println!("\n{channel}: {} item(s)", items.len());
        for item in items {
            println!("  {}", item.text);
            println!("      {}", item.url.unwrap_or_else(|| "(no link)".to_string()));
        }
    }
}