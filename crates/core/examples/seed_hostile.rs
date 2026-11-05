//! Developer tool: fills a SCRATCH data folder with hostile items so you can look at them in the
//! real app. The items go in raw, skipping the cleaning the wisps normally do, which is the worst
//! case: if bad data ever got past the cleaning, the pages must still show it as plain text and
//! nothing may run. It refuses to touch your real data folder.
//!
//!     cargo run -p wisply-core --example seed_hostile -- C:\wisply-scratch

use std::path::PathBuf;

use wisply_core::store::{self, NewItem, WispChanges};
use wisply_core::{paths, runtime, schema};

/// (text, link) pairs. Each text says what to expect, so the screen explains itself.
fn hostile_items() -> Vec<(String, Option<String>)> {
    vec![
        ("<img src=x onerror=alert(1)>".to_string(), None),
        ("<script>alert('script ran')</script>".to_string(), None),
        ("\"><svg onload=alert(1)> {{7*7}} ${7*7}".to_string(), None),
        (
            "JS link: click me (nothing may happen)".to_string(),
            Some("javascript:alert('js link ran')".to_string()),
        ),
        (
            "File link: click me (Calculator must NOT open)".to_string(),
            Some("file:///C:/Windows/System32/calc.exe".to_string()),
        ),
        (
            "Data link: click me (nothing may happen)".to_string(),
            Some("data:text/html,<script>alert('data link ran')</script>".to_string()),
        ),
        (
            "Plain http link: click me (nothing may happen)".to_string(),
            Some("http://example.com/plain-http".to_string()),
        ),
        (
            "Other https site: click me (opens example.com in your browser: a known gap)".to_string(),
            Some("https://example.com/not-a-listed-host".to_string()),
        ),
        (
            "Kitchen sink: line1\nline2\r\nline3\ttab\u{7}bell \u{202E}txt.exe '); DROP TABLE items; -- &lt;b&gt;&amp; <b>bold</b>"
                .to_string(),
            None,
        ),
        (format!("LONG {}", "x".repeat(100_000)), None),
    ]
}

fn main() {
    let Some(arg) = std::env::args().nth(1) else {
        eprintln!("Usage: cargo run -p wisply-core --example seed_hostile -- <scratch folder>");
        std::process::exit(2);
    };
    let dir = PathBuf::from(arg);
    std::fs::create_dir_all(&dir).expect("could not create the folder");

    // Never fill the real data folder with junk.
    if let Ok(appdata) = std::env::var("APPDATA") {
        let real = PathBuf::from(appdata).join("Wisply");
        if dir.canonicalize().ok() == real.canonicalize().ok() {
            eprintln!("That is your real Wisply data folder. Pick a different, scratch folder.");
            std::process::exit(2);
        }
    }

    // Sets the folder up the way the app does on its first start: the database and the three wisps.
    runtime::prepare(&dir).expect("could not prepare the folder");
    let conn = schema::open(&dir.join(paths::DB_FILE)).expect("could not open the database");

    let wisps = store::all_wisps(&conn).expect("could not list the wisps");
    let mut added = 0;
    for (n, wisp) in wisps.iter().enumerate() {
        // Switched on so the Hub shows them; stamped as just checked and a day apart, so the app
        // does not go to the internet and push these items down the list.
        store::set_wisp_active(&conn, &wisp.id, true).expect("could not switch the wisp on");
        store::set_fetch_interval(&conn, &wisp.id, 86_400).expect("could not set the interval");
        store::mark_fetched(&conn, &wisp.id).expect("could not stamp the wisp");

        // The first wisp also gets a hostile name.
        if n == 0 {
            let changes = WispChanges {
                name: Some("<i>Italic?</i><img src=x onerror=alert(1)>".to_string()),
                ..Default::default()
            };
            store::update_wisp(&conn, &wisp.id, &changes).expect("could not rename the wisp");
        }

        let channels = store::wisp_channels(&conn, &wisp.id).expect("could not list the channels");
        let Some(channel) = channels.first() else { continue };
        for (i, (text, url)) in hostile_items().into_iter().enumerate() {
            let item = NewItem {
                channel_id: channel.id.clone(),
                dedupe_key: format!("hostile-{}-{}", wisp.id, i),
                text,
                url,
                ..Default::default()
            };
            if store::add_item(&conn, &item).expect("could not add an item") {
                added += 1;
            }
        }
    }
    println!("Filled {} with {} hostile item(s) across {} wisp(s).", dir.display(), added, wisps.len());
}