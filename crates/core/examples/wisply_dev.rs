//! Developer tool: look at and change your Wisply data from the command line, until the Hub can do
//! it (R5). It works on the same data folder the app uses, so to practise safely first point both
//! at a scratch folder:
//!
//!     set WISPLY_DATA_DIR=C:\temp\wisply-test
//!
//! Commands (run with `cargo run -p wisply-core --example wisply_dev -- <command>`):
//!
//!     status                          every wisp: on/off, check interval, last check, unread
//!     on <wisp>                       switch a wisp on  (game_deals, github or companion)
//!     off <wisp>                      switch a wisp off
//!     add "text" --in <minutes> [--repeat daily|weekly|monthly|yearly]
//!     add "text" --at "2026-10-28 09:30" [--repeat ...]
//!     idea "text"                     drop an idea in the jar
//!     list                            every reminder and idea, with its number
//!     delete <number>                 delete a reminder or idea
//!     check                           run Companion's check right now
//!     mood                            how pip feels right now

use chrono::{Duration, Local};
use wisply_core::wisps::companion;
use wisply_core::wisps::companion_voice as voice;
use wisply_core::{account, open_in_dir, paths, runtime, store};

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

/// The value after a flag, for example `--in 5` gives `Some("5")`.
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).map(String::as_str)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("");
    let dir = paths::data_dir().map_err(|e| e.to_string())?;
    println!("Data folder: {}", dir.display());
    let conn = open_in_dir(&dir).map_err(|e| e.to_string())?.conn;
    runtime::register_wisps(&conn).map_err(|e| e.to_string())?;
    let now = Local::now().naive_local();

    match command {
        "status" => {
            for wisp in store::all_wisps(&conn).map_err(|e| e.to_string())? {
                let unread = store::wisp_unread(&conn, &wisp.id).map_err(|e| e.to_string())?;
                println!(
                    "{:<11} {:<12} {:<4} every {:>5}s   last check {} (UTC)   unread {}",
                    wisp.id,
                    wisp.name,
                    if wisp.active { "ON" } else { "off" },
                    wisp.fetch_interval_seconds,
                    wisp.last_fetched_at.as_deref().unwrap_or("never"),
                    unread
                );
            }
        }
        "on" | "off" => {
            let id = args.get(1).ok_or("which wisp? (game_deals, github or companion)")?;
            if store::get_wisp(&conn, id).map_err(|e| e.to_string())?.is_none() {
                return Err(format!("there is no wisp called {id:?}"));
            }
            store::set_wisp_active(&conn, id, command == "on").map_err(|e| e.to_string())?;
            println!("{id} is now {}.", if command == "on" { "ON" } else { "off" });
        }
        "add" => {
            let text = args.get(1).ok_or("what should I remind you about?")?;
            let when = match (flag(&args, "--in"), flag(&args, "--at")) {
                (Some(minutes), _) => {
                    let minutes: i64 = minutes.parse().map_err(|_| "--in needs a number of minutes")?;
                    let due = now + Duration::minutes(minutes);
                    (due.format("%Y-%m-%d").to_string(), due.format("%H:%M").to_string())
                }
                (None, Some(at)) => {
                    let (date, time) = at.split_once(' ').ok_or("--at needs \"YYYY-MM-DD HH:MM\"")?;
                    (date.to_string(), time.to_string())
                }
                _ => return Err("say when: --in <minutes> or --at \"YYYY-MM-DD HH:MM\"".into()),
            };
            companion::create_reminder(&conn, text, Some(&when.0), Some(&when.1), flag(&args, "--repeat"), now)
                .map_err(|e| e.to_string())?;
            println!("{} (due {} {})", voice::SAVED_REMINDER, when.0, when.1);
        }
        "idea" => {
            let text = args.get(1).ok_or("what's the idea?")?;
            companion::create_idea(&conn, text, now).map_err(|e| e.to_string())?;
            println!("{}", voice::SAVED_IDEA);
        }
        "list" => {
            let notes = companion::list_notes(&conn).map_err(|e| e.to_string())?;
            if notes.is_empty() {
                println!("No notes yet.");
            }
            for note in notes {
                let when = note.due_at.map(|d| format!(" due {d}")).unwrap_or_default();
                let repeat = if note.repeat == "none" { String::new() } else { format!(" ({})", note.repeat) };
                let done = if note.done { " [done]" } else { "" };
                println!("{:>3}  {:<8}{done}{when}{repeat}  {}", note.id, note.kind, note.text);
            }
        }
        "delete" => {
            let id: i64 = args.get(1).and_then(|a| a.parse().ok()).ok_or("delete needs the note's number (see `list`)")?;
            companion::delete_note(&conn, id).map_err(|e| e.to_string())?;
            println!("Deleted note {id}.");
        }
        "check" => {
            let faces: Vec<String> = runtime::FACES.iter().map(|f| f.to_string()).collect();
            companion::check(&conn, now, &faces).map_err(|e| e.to_string())?;
            println!(
                "Checked. Unread in Today: {}, in Idea of the day: {}.",
                store::unread_count(&conn, companion::TODAY).map_err(|e| e.to_string())?,
                store::unread_count(&conn, companion::IDEAS).map_err(|e| e.to_string())?
            );
        }
        "mood" => {
            let name = account::get_name(&dir);
            let feeling = companion::describe_mood(&conn, now, name.as_deref()).map_err(|e| e.to_string())?;
            println!("{} / {}\n{}", feeling.mood, feeling.greeting, feeling.line);
        }
        _ => return Err("commands: status, on, off, add, idea, list, delete, check, mood (see the top of examples/wisply_dev.rs)".into()),
    }
    Ok(())
}
