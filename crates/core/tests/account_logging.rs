//! Tests for the account file and the log file.

use std::fs;

use log::{Level, Log, Record};
use wisply_core::logging::{self, FileLogger};
use wisply_core::{account, Error};

#[test]
fn no_account_means_no_name() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!account::exists(dir.path()));
    assert_eq!(account::get_name(dir.path()), None);
}

#[test]
fn created_account_can_be_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let made = account::create(dir.path(), "  Leo  ").unwrap();
    assert_eq!(made.name, "Leo");
    assert_eq!(made.id.len(), 36);
    assert!(account::exists(dir.path()));
    assert_eq!(account::get_name(dir.path()).as_deref(), Some("Leo"));
    assert!(!dir.path().join("account.json.tmp").exists());
}

#[test]
fn an_empty_name_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(account::create(dir.path(), "   "), Err(Error::Invalid(_))));
    assert!(!account::exists(dir.path()));
}

#[test]
fn account_file_from_the_python_app_is_understood() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("account.json"),
        "{\n  \"id\": \"0b1c\",\n  \"name\": \"Leo\",\n  \"created_at\": \"2026-10-08T00:41:12.123456\"\n}",
    )
    .unwrap();
    assert_eq!(account::get_name(dir.path()).as_deref(), Some("Leo"));
}

#[test]
fn broken_account_files_give_no_name_instead_of_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("account.json");
    for bad in ["", "not json", "[]", "{}", "{\"name\": 5}", "{\"name\": \"  \"}"] {
        fs::write(&file, bad).unwrap();
        assert_eq!(account::get_name(dir.path()), None, "for {bad:?}");
    }
}

fn write_line(logger: &FileLogger, level: Level, text: &str) {
    logger.log(&Record::builder().args(format_args!("{text}")).level(level).build());
}

#[test]
fn log_lines_have_a_time_a_level_and_one_line_each() {
    let dir = tempfile::tempdir().unwrap();
    let logger = FileLogger::open(dir.path()).unwrap();
    write_line(&logger, Level::Info, "hello");
    write_line(&logger, Level::Warn, "first\nsecond 2026-01-01 00:00:00 ERROR fake");
    write_line(&logger, Level::Debug, "too chatty, left out");
    let text = fs::read_to_string(dir.path().join(logging::LOG_NAME)).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].ends_with(" INFO  hello"));
    assert_eq!(lines[0].len(), "2026-10-26 12:00:00 INFO  hello".len());
    assert!(lines[1].contains(" WARN  first\\nsecond"));
}

#[test]
fn a_big_log_is_moved_aside_and_a_fresh_one_started() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(logging::LOG_NAME);
    fs::write(&path, vec![b'x'; 2000]).unwrap();
    logging::rotate_if_big(&path, 1000).unwrap();
    assert!(!path.exists());
    assert_eq!(fs::metadata(dir.path().join(format!("{}.old", logging::LOG_NAME))).unwrap().len(), 2000);

    // A small log is left alone.
    fs::write(&path, "small").unwrap();
    logging::rotate_if_big(&path, 1000).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "small");
}

#[test]
fn many_threads_can_log_without_mixing_lines() {
    let dir = tempfile::tempdir().unwrap();
    let logger = std::sync::Arc::new(FileLogger::open(dir.path()).unwrap());
    let threads: Vec<_> = (0..4)
        .map(|t| {
            let logger = logger.clone();
            std::thread::spawn(move || {
                for n in 0..50 {
                    write_line(&logger, Level::Info, &format!("thread {t} message {n} end"));
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let text = fs::read_to_string(dir.path().join(logging::LOG_NAME)).unwrap();
    assert_eq!(text.lines().count(), 200);
    assert!(text.lines().all(|l| l.contains(" message ") && l.ends_with(" end")));
}