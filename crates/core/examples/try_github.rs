//! Developer tool: runs the real GitHub check against the real GitHub and prints what it found.
//! It uses a throw-away database in memory, so it changes nothing on disk. The check makes five
//! searches with a 2 second pause between them, so it takes about ten seconds.
//!
//!     cargo run -p wisply-core --example try_github

use wisply_core::scheduler::Fetcher;
use wisply_core::wisps::github::{self, GithubFetcher};
use wisply_core::{schema, store};

fn main() {
    let conn = schema::open_in_memory().expect("could not open a database");
    github::register_wisp(&conn).expect("could not register the wisp");

    println!("Asking GitHub (about ten seconds)...");
    if let Err(error) = GithubFetcher::new().fetch(&conn) {
        eprintln!("The check failed: {error}");
        std::process::exit(1);
    }
    for channel in store::wisp_channels(&conn, "github").expect("could not read channels") {
        let items = store::items(&conn, &channel.id, None).expect("could not read items");
        println!("\n{} ({}): {} item(s)", channel.title, channel.id, items.len());
        for item in items {
            println!("  {}", item.text);
            println!("      {}", item.url.unwrap_or_else(|| "(no link)".to_string()));
        }
    }
}