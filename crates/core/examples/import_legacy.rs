//! Developer tool: import an old `data.db` into a new database file and print what happened.
//!
//!     cargo run -p wisply-core --example import_legacy -- <old data.db> <new wisply.db>
//!
//! The old file is only read. The new file must not exist yet (or must not contain any wisps).

use std::path::PathBuf;
use std::process::ExitCode;

use wisply_core::{legacy, schema};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(old), Some(new)) = (args.next(), args.next()) else {
        eprintln!("usage: import_legacy <old data.db> <new wisply.db>");
        return ExitCode::from(2);
    };
    let (old, new) = (PathBuf::from(old), PathBuf::from(new));

    let result = schema::open(&new).and_then(|conn| legacy::import(&old, &conn));
    match result {
        Ok(report) => {
            println!("Imported:");
            println!("  wisps:    {}", report.wisps);
            println!("  channels: {}", report.channels);
            println!("  items:    {}", report.items);
            println!("  notes:    {}", report.notes);
            println!("Skipped:");
            println!("  channels without a wisp:    {}", report.skipped_channels);
            println!("  items without a channel:    {}", report.skipped_items);
            println!("  duplicate items:            {}", report.skipped_duplicates);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Import failed: {error}");
            ExitCode::FAILURE
        }
    }
}