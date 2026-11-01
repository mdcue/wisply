//! Tests for Companion ("pip") and its voice. Time is always passed in, so nothing here depends on
//! today's date. Unless a test says otherwise, "now" is Monday 26 October 2026, 10:00.

use chrono::{NaiveDate, NaiveDateTime};
use rusqlite::Connection;
use wisply_core::scheduler::{self, Fetch, Outcome, Scheduler};
use wisply_core::wisps::companion::{self, CompanionFetcher, NoteError, Repeat, IDEAS, TODAY};
use wisply_core::wisps::companion_voice::{self as voice, Mood, Problem};
use wisply_core::{schema, store};

fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, month, day).unwrap().and_hms_opt(hour, minute, 0).unwrap()
}

fn now() -> NaiveDateTime {
    at(2026, 10, 26, 10, 0)
}

fn setup() -> Connection {
    let conn = schema::open_in_memory().unwrap();
    companion::register_wisp(&conn).unwrap();
    store::set_wisp_active(&conn, "companion", true).unwrap();
    conn
}

fn texts(conn: &Connection, channel: &str) -> Vec<String> {
    store::items(conn, channel, None).unwrap().into_iter().map(|i| i.text).collect()
}

/// A reminder put straight into the database, so it can be in the past or odd.
fn reminder(conn: &Connection, text: &str, due: NaiveDateTime, repeat: Repeat) -> i64 {
    companion::add_reminder(conn, text, due, repeat, now()).unwrap();
    conn.last_insert_rowid()
}

fn due_at(conn: &Connection, id: i64) -> String {
    conn.query_row("SELECT due_at FROM companion_notes WHERE id = ?1", [id], |r| r.get(0)).unwrap()
}

fn message(error: NoteError) -> String {
    error.to_string()
}

fn expressions(conn: &Connection) -> String {
    store::get_wisp(conn, "companion").unwrap().unwrap().expression
}

// --- the wisp itself ---

#[test]
fn registering_creates_pip_with_its_channels_and_a_one_minute_interval() {
    let conn = schema::open_in_memory().unwrap();
    companion::register_wisp(&conn).unwrap();
    companion::register_wisp(&conn).unwrap();
    let wisp = store::get_wisp(&conn, "companion").unwrap().unwrap();
    assert_eq!((wisp.name.as_str(), wisp.color.as_str()), ("pip", "#ff7a59"));
    assert_eq!(wisp.fetch_interval_seconds, 60);
    let channels = store::wisp_channels(&conn, "companion").unwrap();
    let shown: Vec<_> = channels.iter().map(|c| (c.id.as_str(), c.title.as_str())).collect();
    assert_eq!(shown, [("co_today", "Today"), ("co_ideas", "Idea of the day")]);
    assert_eq!(companion::FETCH_INTERVALS, [60, 300, 900, 1800, 3600]);
}

#[test]
fn a_changed_interval_is_kept_when_registering_again() {
    let conn = setup();
    store::set_fetch_interval(&conn, "companion", 900).unwrap();
    companion::register_wisp(&conn).unwrap();
    assert_eq!(store::get_wisp(&conn, "companion").unwrap().unwrap().fetch_interval_seconds, 900);
}

// --- the voice ---

#[test]
fn greetings_match_what_the_python_app_said() {
    let cases = [
        (at(2026, 10, 26, 10, 0), Mood::Calm, "Rise and shine, Sam!", "Rise and shine!"),
        (at(2026, 10, 27, 14, 0), Mood::Worried, "Afternoon, Sam! Still going strong?", "Afternoon! Still going strong?"),
        (at(2026, 10, 28, 19, 0), Mood::Happy, "Evening, Sam! Nearly time to unwind.", "Evening! Nearly time to unwind."),
        (at(2026, 10, 29, 23, 0), Mood::Sleepy, "Still up, Sam?", "Still up?"),
        (at(2026, 10, 30, 3, 0), Mood::Sleepy, "It's late, Sam. Don't forget to rest!", "It's late. Don't forget to rest!"),
        (at(2026, 10, 31, 9, 0), Mood::Calm, "Morning, Sam! Fresh day, fresh start.", "Morning! Fresh day, fresh start."),
        (at(2026, 11, 1, 9, 0), Mood::Calm, "Rise and shine, Sam!", "Rise and shine!"),
    ];
    for (when, mood, with_name, without) in cases {
        assert_eq!(voice::greeting(when, mood, Some("Sam")), with_name);
        assert_eq!(voice::greeting(when, mood, None), without);
    }
}

#[test]
fn a_greeting_holds_all_day_and_changes_between_days() {
    let all_day: Vec<_> = [6, 8, 11].iter().map(|h| voice::greeting(at(2026, 10, 26, *h, 30), Mood::Calm, None)).collect();
    assert!(all_day.windows(2).all(|w| w[0] == w[1]));
    let across_days: std::collections::HashSet<_> =
        (1..=9).map(|d| voice::greeting(at(2026, 10, d, 9, 0), Mood::Calm, Some("Sam"))).collect();
    assert_eq!(across_days.len(), 3);
}

#[test]
fn a_blank_name_is_left_out() {
    let plain = voice::greeting(now(), Mood::Calm, None);
    assert_eq!(voice::greeting(now(), Mood::Calm, Some("   ")), plain);
    assert_eq!(voice::greeting(now(), Mood::Calm, Some("  Sam  ")), "Rise and shine, Sam!");
}

#[test]
fn the_part_of_day_follows_the_clock_and_the_mood() {
    assert_eq!(voice::part_of_day(at(2026, 10, 26, 11, 59), Mood::Calm), "morning");
    assert_eq!(voice::part_of_day(at(2026, 10, 26, 12, 0), Mood::Calm), "afternoon");
    assert_eq!(voice::part_of_day(at(2026, 10, 26, 16, 59), Mood::Calm), "afternoon");
    assert_eq!(voice::part_of_day(at(2026, 10, 26, 17, 0), Mood::Calm), "evening");
    assert_eq!(voice::part_of_day(at(2026, 10, 26, 12, 0), Mood::Sleepy), "night");
}

#[test]
fn the_words_are_pips_own() {
    assert_eq!(voice::SAVED_REMINDER, "Got it! I'll remind you.");
    assert_eq!(voice::SAVED_IDEA, "Tucked into the jar!");
    assert_eq!(voice::EMPTY_REMINDERS, "Nothing coming up. Enjoy the quiet!");
    assert_eq!(voice::EMPTY_IDEAS, "The jar is empty. Drop an idea in!");
    assert_eq!(voice::COMPANION_OFF, "I'm switched off right now. Turn me on in the Hub and I'll keep that for you.");
    assert_eq!(voice::IDEA_WINDOW, "A new idea!");
    assert_eq!(voice::REMINDER_WINDOW, "Remind me...");
    assert_eq!(voice::IDEA_PLACEHOLDER, "What's the idea?");
    assert_eq!(voice::REMINDER_PLACEHOLDER, "What should I remind you about?");
    assert_eq!(voice::due_item("Call mum", None), "It's time: Call mum");
    assert_eq!(voice::due_item("Call mum", Some("Fri 23 Oct, 10:00")), "Better late than never: Call mum (was due Fri 23 Oct, 10:00)");
    assert_eq!(voice::eve_item("Pay rent", "09:00"), "Heads-up! Tomorrow at 09:00: Pay rent");
    assert_eq!(voice::idea_item("Learn to juggle"), "Remember this one? Learn to juggle");
    assert_eq!(voice::new_items("pip"), "pip has something new for you");
}

#[test]
fn every_friendly_error_reads_as_before() {
    assert_eq!(voice::error(Problem::Empty), "Oops, there's nothing written yet.");
    assert_eq!(voice::error(Problem::TooLong { limit: 300 }), "That's a bit of a mouthful! Keep it under 300 characters.");
    assert_eq!(voice::error(Problem::Repeat), "Pick how often it repeats.");
    assert_eq!(voice::error(Problem::When), "Pick a date and a time, please.");
    assert_eq!(voice::error(Problem::Past), "That time has already gone by. Pick one a little later.");
    assert_eq!(voice::error(Problem::NoteGone), "Hmm, I can't find that note. Maybe it's already gone.");
    assert_eq!(voice::error(Problem::ReminderGone), "Hmm, I can't find that reminder.");
    assert_eq!(voice::error(Problem::Snooze), "Pick how long to snooze for.");
}

#[test]
fn mood_lines_count_the_reminders() {
    assert_eq!(voice::mood_line(Mood::Sleepy, 1, 0, 0), "I'm drowsy, but a reminder is still waiting for you.");
    assert_eq!(voice::mood_line(Mood::Sleepy, 3, 0, 0), "I'm drowsy, but 3 reminders are still waiting for you.");
    assert_eq!(voice::mood_line(Mood::Sleepy, 0, 0, 0), "Zzz... all quiet. I'll keep an eye out while you rest.");
    assert_eq!(voice::mood_line(Mood::Worried, 1, 1, 0), "A reminder is waiting on you. Mark it done or snooze it and I'll relax.");
    assert_eq!(voice::mood_line(Mood::Worried, 2, 2, 0), "2 reminders are waiting on you. Mark them done or snooze them and I'll relax.");
    assert_eq!(voice::mood_line(Mood::Worried, 0, 1, 0), "There's a heads-up waiting in Today. Have a look when you can.");
    assert_eq!(voice::mood_line(Mood::Happy, 0, 0, 0), "All caught up! I'm very proud of you.");
    assert_eq!(voice::mood_line(Mood::Calm, 0, 0, 1), "Your idea of the day is waiting. No rush!");
}

#[test]
fn each_mood_tries_its_faces_in_order() {
    assert_eq!(Mood::Sleepy.expressions(), ["sleepy", "sleeping", "tired", "yawning", "blank"]);
    assert_eq!(Mood::Worried.expressions(), ["worried", "concerned", "anxious", "nervous"]);
    assert_eq!(Mood::Happy.expressions(), ["happy", "laughing", "smiling"]);
    assert_eq!(Mood::Calm.expressions(), ["content", "smiling", "happy"]);
}

// --- saving notes ---

#[test]
fn a_reminder_is_saved_trimmed_and_in_the_stored_format() {
    let conn = setup();
    companion::create_reminder(&conn, "  Call mum  ", Some("2026-10-26"), Some("10:00"), Some("weekly"), now()).unwrap();
    let notes = companion::list_notes(&conn).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].text, "Call mum");
    assert_eq!(notes[0].kind, "reminder");
    assert_eq!(notes[0].due_at.as_deref(), Some("2026-10-26 10:00"));
    assert_eq!(notes[0].repeat, "weekly");
    assert!(!notes[0].done);
}

#[test]
fn a_blank_or_missing_repeat_means_none() {
    let conn = setup();
    companion::create_reminder(&conn, "a", Some("2026-10-27"), Some("10:00"), None, now()).unwrap();
    companion::create_reminder(&conn, "b", Some("2026-10-27"), Some("10:00"), Some("  "), now()).unwrap();
    assert!(companion::list_notes(&conn).unwrap().iter().all(|n| n.repeat == "none"));
}

#[test]
fn bad_notes_are_refused_with_a_friendly_sentence_and_nothing_is_saved() {
    let conn = setup();
    let go = |text: &str, date: Option<&str>, time: Option<&str>, repeat: Option<&str>| {
        message(companion::create_reminder(&conn, text, date, time, repeat, now()).unwrap_err())
    };
    assert_eq!(go("   ", Some("2026-10-27"), Some("10:00"), None), "Oops, there's nothing written yet.");
    assert_eq!(
        go(&"x".repeat(301), Some("2026-10-27"), Some("10:00"), None),
        "That's a bit of a mouthful! Keep it under 300 characters."
    );
    assert_eq!(go("a", Some("bad"), Some("bad"), None), "Pick a date and a time, please.");
    assert_eq!(go("a", None, Some("10:00"), None), "Pick a date and a time, please.");
    assert_eq!(go("a", Some("2026-10-27"), None, None), "Pick a date and a time, please.");
    assert_eq!(go("a", Some("2020-01-01"), Some("10:00"), None), "That time has already gone by. Pick one a little later.");
    assert_eq!(go("a", Some("2026-10-30"), Some("10:00"), Some("montly")), "Pick how often it repeats.");
    assert!(companion::list_notes(&conn).unwrap().is_empty());
}

#[test]
fn the_length_limit_counts_characters_and_a_minute_of_grace_is_allowed() {
    let conn = setup();
    companion::create_reminder(&conn, &"é".repeat(300), Some("2026-10-27"), Some("10:00"), None, now()).unwrap();
    companion::create_reminder(&conn, "just now", Some("2026-10-26"), Some("09:59"), None, now()).unwrap();
    assert!(companion::create_reminder(&conn, "too old", Some("2026-10-26"), Some("09:58"), None, now()).is_err());
}

#[test]
fn problems_can_be_told_apart_from_database_trouble() {
    let conn = setup();
    let error = companion::create_idea(&conn, "  ", now()).unwrap_err();
    assert!(matches!(error, NoteError::Problem(Problem::Empty)));
}

#[test]
fn ideas_are_saved_and_listed_newest_first() {
    let conn = setup();
    companion::create_idea(&conn, "  Learn to juggle ", now()).unwrap();
    companion::create_idea(&conn, "Paint the shed", now()).unwrap();
    let ideas = companion::list_ideas(&conn).unwrap();
    assert_eq!(ideas.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(), ["Paint the shed", "Learn to juggle"]);
    companion::delete_note(&conn, ideas[0].id).unwrap();
    assert_eq!(companion::list_ideas(&conn).unwrap().len(), 1);
}

#[test]
fn reminders_are_listed_soonest_first_with_friendly_labels() {
    let conn = setup();
    reminder(&conn, "later", at(2026, 10, 30, 18, 0), Repeat::Daily);
    reminder(&conn, "tomorrow", at(2026, 10, 27, 8, 30), Repeat::None);
    reminder(&conn, "today", at(2026, 10, 26, 14, 0), Repeat::None);
    reminder(&conn, "yesterday", at(2026, 10, 25, 9, 0), Repeat::None);
    let finished = reminder(&conn, "finished", at(2026, 10, 26, 11, 0), Repeat::None);
    companion::complete_reminder(&conn, finished, now()).unwrap();
    companion::add_idea(&conn, "not a reminder", now()).unwrap();

    let list = companion::list_reminders(&conn, now()).unwrap();
    let shown: Vec<_> = list.iter().map(|r| (r.text.as_str(), r.label.as_str(), r.status)).collect();
    assert_eq!(
        shown,
        [
            ("yesterday", "Yesterday 09:00", "due"),
            ("today", "Today 14:00", "upcoming"),
            ("tomorrow", "Tomorrow 08:30", "upcoming"),
            ("later", "Fri 30 Oct, 18:00", "upcoming"),
        ]
    );
    assert_eq!((list[3].date.as_str(), list[3].time.as_str(), list[3].repeat.as_str()), ("2026-10-30", "18:00", "daily"));
}

// --- firing reminders ---

#[test]
fn a_reminder_fires_when_due_and_only_once() {
    let conn = setup();
    companion::create_reminder(&conn, "Call mum", Some("2026-10-26"), Some("10:00"), None, now()).unwrap();
    companion::check(&conn, now(), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY), ["It's time: Call mum"]);
    companion::check(&conn, at(2026, 10, 26, 10, 1), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY).len(), 1);
    // It stays in the list as due until the user deals with it.
    assert_eq!(companion::list_reminders(&conn, now()).unwrap()[0].status, "due");
}

#[test]
fn a_reminder_that_is_not_due_yet_says_nothing() {
    let conn = setup();
    companion::create_reminder(&conn, "Dentist", Some("2026-10-28"), Some("10:00"), None, now()).unwrap();
    companion::check(&conn, now(), &[]).unwrap();
    assert!(texts(&conn, TODAY).is_empty());
}

#[test]
fn a_late_reminder_says_when_it_was_really_due() {
    let conn = setup();
    reminder(&conn, "Old thing", at(2026, 10, 26, 11, 0), Repeat::None);
    // 60 minutes late is still on time; one more minute is late.
    companion::check(&conn, at(2026, 10, 26, 12, 0), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY), ["It's time: Old thing"]);

    reminder(&conn, "Older thing", at(2026, 10, 26, 8, 0), Repeat::None);
    companion::check(&conn, at(2026, 10, 26, 12, 1), &[]).unwrap();
    assert!(texts(&conn, TODAY).contains(&"Better late than never: Older thing (was due Mon 26 Oct, 08:00)".to_string()));
}

#[test]
fn the_evening_before_a_heads_up_appears_once() {
    let conn = setup();
    reminder(&conn, "Pay rent", at(2026, 10, 27, 9, 0), Repeat::None);
    reminder(&conn, "Far away", at(2026, 10, 28, 9, 0), Repeat::None);
    companion::check(&conn, now(), &[]).unwrap();
    companion::check(&conn, at(2026, 10, 26, 18, 0), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY), ["Heads-up! Tomorrow at 09:00: Pay rent"]);
}

#[test]
fn a_note_with_line_breaks_becomes_one_line_in_the_feed() {
    let conn = setup();
    reminder(&conn, "line1\nline2   end", now(), Repeat::None);
    companion::check(&conn, now(), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY), ["It's time: line1 line2 end"]);
}

#[test]
fn a_dismissed_reminder_item_does_not_come_back() {
    let conn = setup();
    reminder(&conn, "Call mum", now(), Repeat::None);
    companion::check(&conn, now(), &[]).unwrap();
    let id = store::items(&conn, TODAY, None).unwrap()[0].id;
    store::dismiss_item(&conn, id).unwrap();
    companion::check(&conn, at(2026, 10, 26, 10, 5), &[]).unwrap();
    assert!(texts(&conn, TODAY).is_empty());
}

// --- repeating ---

#[test]
fn a_daily_reminder_fires_then_moves_to_tomorrow_and_warns_the_evening_before() {
    let conn = setup();
    let id = reminder(&conn, "Stretch", at(2026, 10, 26, 9, 0), Repeat::Daily);
    companion::check(&conn, now(), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY), ["It's time: Stretch"]);
    assert_eq!(due_at(&conn, id), "2026-10-27 09:00");
    companion::check(&conn, at(2026, 10, 26, 10, 1), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY)[0], "Heads-up! Tomorrow at 09:00: Stretch");
    assert_eq!(texts(&conn, TODAY).len(), 2);
}

#[test]
fn a_repeating_reminder_missed_for_weeks_fires_once() {
    let conn = setup();
    let id = reminder(&conn, "Water plants", at(2026, 10, 5, 9, 0), Repeat::Weekly);
    companion::check(&conn, at(2026, 10, 26, 12, 0), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY), ["Better late than never: Water plants (was due Mon 05 Oct, 09:00)"]);
    assert_eq!(due_at(&conn, id), "2026-11-02 09:00");
}

#[test]
fn months_and_years_that_are_too_short_pull_the_day_back_like_before() {
    let conn = setup();
    let monthly = reminder(&conn, "Rent", at(2026, 1, 31, 9, 0), Repeat::Monthly);
    companion::complete_reminder(&conn, monthly, at(2026, 1, 31, 8, 0)).unwrap();
    assert_eq!(due_at(&conn, monthly), "2026-02-28 09:00");

    let yearly = reminder(&conn, "Leap day", at(2024, 2, 29, 9, 0), Repeat::Yearly);
    companion::complete_reminder(&conn, yearly, at(2024, 2, 29, 8, 0)).unwrap();
    assert_eq!(due_at(&conn, yearly), "2025-02-28 09:00");
}

#[test]
fn a_reminder_with_an_unknown_repeat_in_the_database_counts_as_one_off() {
    let conn = setup();
    let id = reminder(&conn, "Odd", now(), Repeat::None);
    conn.execute("UPDATE companion_notes SET repeat_rule = 'fortnightly' WHERE id = ?1", [id]).unwrap();
    companion::check(&conn, now(), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY), ["It's time: Odd"]);
    assert_eq!(due_at(&conn, id), "2026-10-26 10:00");
    assert_eq!(companion::list_reminders(&conn, now()).unwrap()[0].repeat, "none");
}

// --- done, snooze, edit ---

#[test]
fn finishing_a_one_off_removes_it_and_finishing_a_repeating_one_skips_this_round() {
    let conn = setup();
    let once = reminder(&conn, "once", at(2026, 10, 26, 9, 0), Repeat::None);
    let daily = reminder(&conn, "daily", at(2026, 10, 26, 9, 0), Repeat::Daily);
    companion::complete_reminder(&conn, once, now()).unwrap();
    companion::complete_reminder(&conn, daily, now()).unwrap();
    let left: Vec<_> = companion::list_reminders(&conn, now()).unwrap().into_iter().map(|r| r.text).collect();
    assert_eq!(left, ["daily"]);
    assert_eq!(due_at(&conn, daily), "2026-10-27 09:00");
    // An unknown id is simply ignored.
    companion::complete_reminder(&conn, 9999, now()).unwrap();
}

#[test]
fn snoozing_a_one_off_moves_it_and_brings_it_back_if_finished() {
    let conn = setup();
    let id = reminder(&conn, "Call mum", at(2026, 10, 26, 9, 0), Repeat::None);
    companion::snooze_reminder(&conn, id, "10m", now()).unwrap();
    assert_eq!(due_at(&conn, id), "2026-10-26 10:10");
    companion::snooze_reminder(&conn, id, "1h", now()).unwrap();
    assert_eq!(due_at(&conn, id), "2026-10-26 11:00");
    companion::snooze_reminder(&conn, id, "tomorrow", now()).unwrap();
    assert_eq!(due_at(&conn, id), "2026-10-27 09:00");

    companion::complete_reminder(&conn, id, now()).unwrap();
    assert!(companion::list_reminders(&conn, now()).unwrap().is_empty());
    companion::snooze_reminder(&conn, id, "10m", now()).unwrap();
    assert_eq!(companion::list_reminders(&conn, now()).unwrap().len(), 1);
}

#[test]
fn snoozing_a_repeating_reminder_makes_a_one_off_copy_and_keeps_the_schedule() {
    let conn = setup();
    let id = reminder(&conn, "Stretch", at(2026, 10, 26, 9, 0), Repeat::Daily);
    companion::snooze_reminder(&conn, id, "1h", now()).unwrap();
    assert_eq!(due_at(&conn, id), "2026-10-27 09:00");
    let list = companion::list_reminders(&conn, now()).unwrap();
    assert_eq!(list.len(), 2);
    let copy = list.iter().find(|r| r.id != id).unwrap();
    assert_eq!((copy.text.as_str(), copy.time.as_str(), copy.repeat.as_str()), ("Stretch", "11:00", "none"));
}

#[test]
fn snooze_mistakes_get_friendly_answers() {
    let conn = setup();
    let id = reminder(&conn, "x", now(), Repeat::None);
    assert_eq!(message(companion::snooze_reminder(&conn, id, "zzz", now()).unwrap_err()), "Pick how long to snooze for.");
    assert_eq!(message(companion::snooze_reminder(&conn, 9999, "10m", now()).unwrap_err()), "Hmm, I can't find that reminder.");
    // The snooze choice is checked first, as before.
    assert_eq!(message(companion::snooze_reminder(&conn, 9999, "zzz", now()).unwrap_err()), "Pick how long to snooze for.");
    // An idea is not a reminder.
    companion::add_idea(&conn, "idea", now()).unwrap();
    let idea = conn.last_insert_rowid();
    assert!(companion::snooze_reminder(&conn, idea, "10m", now()).is_err());
}

#[test]
fn editing_an_idea_only_changes_its_text() {
    let conn = setup();
    companion::add_idea(&conn, "old", now()).unwrap();
    let id = conn.last_insert_rowid();
    companion::update_note(&conn, id, "  new words ", None, None, None, now()).unwrap();
    assert_eq!(companion::list_ideas(&conn).unwrap()[0].text, "new words");
    let blank = companion::update_note(&conn, id, " ", None, None, None, now()).unwrap_err();
    assert_eq!(message(blank), "Oops, there's nothing written yet.");
    assert_eq!(message(companion::update_note(&conn, 9999, "hi", None, None, None, now()).unwrap_err()), "Hmm, I can't find that note. Maybe it's already gone.");
}

#[test]
fn editing_a_reminder_can_keep_a_past_time_but_not_pick_a_new_one() {
    let conn = setup();
    let id = reminder(&conn, "Pay rent", at(2026, 10, 20, 9, 0), Repeat::None);
    companion::complete_reminder(&conn, id, now()).unwrap();
    // Same time, new text and repeat: allowed, and the finished reminder comes back.
    companion::update_note(&conn, id, "Pay the rent", Some("2026-10-20"), Some("09:00"), Some("monthly"), now()).unwrap();
    let list = companion::list_reminders(&conn, now()).unwrap();
    assert_eq!((list[0].text.as_str(), list[0].repeat.as_str()), ("Pay the rent", "monthly"));
    // A different time in the past is refused; one in the future is fine.
    let past = companion::update_note(&conn, id, "x", Some("2026-10-21"), Some("09:00"), None, now()).unwrap_err();
    assert_eq!(message(past), "That time has already gone by. Pick one a little later.");
    companion::update_note(&conn, id, "x", Some("2026-11-01"), Some("09:00"), None, now()).unwrap();
    assert_eq!(due_at(&conn, id), "2026-11-01 09:00");
    let repeat = companion::update_note(&conn, id, "x", Some("2026-11-01"), Some("09:00"), Some("montly"), now()).unwrap_err();
    assert_eq!(message(repeat), "Pick how often it repeats.");
}

// --- the idea of the day ---

#[test]
fn the_idea_of_the_day_waits_for_the_morning_and_comes_once_a_day() {
    let conn = setup();
    companion::add_idea(&conn, "Learn to juggle", now()).unwrap();
    companion::check(&conn, at(2026, 10, 26, 5, 59), &[]).unwrap();
    assert!(texts(&conn, IDEAS).is_empty());
    companion::check(&conn, at(2026, 10, 26, 6, 0), &[]).unwrap();
    assert_eq!(texts(&conn, IDEAS), ["Remember this one? Learn to juggle"]);
    companion::check(&conn, at(2026, 10, 26, 15, 0), &[]).unwrap();
    assert_eq!(texts(&conn, IDEAS).len(), 1);
    let shown: String = conn.query_row("SELECT last_shown FROM companion_notes", [], |r| r.get(0)).unwrap();
    assert_eq!(shown, "2026-10-26 06:00");
    companion::check(&conn, at(2026, 10, 27, 7, 0), &[]).unwrap();
    assert_eq!(texts(&conn, IDEAS).len(), 2);
}

#[test]
fn every_idea_gets_a_turn_before_any_repeats() {
    let conn = setup();
    for idea in ["A", "B", "C"] {
        companion::add_idea(&conn, idea, now()).unwrap();
    }
    let mut shown = Vec::new();
    for day in 26..=29 {
        let moment = at(2026, 10, day, 8, 0);
        companion::check(&conn, moment, &[]).unwrap();
        shown.push(texts(&conn, IDEAS)[0].clone());
    }
    let mut first_three = shown[..3].to_vec();
    first_three.sort();
    assert_eq!(first_three, ["Remember this one? A", "Remember this one? B", "Remember this one? C"]);
    // The fourth day starts the round again with the one shown longest ago.
    assert_eq!(shown[3], shown[0]);
}

#[test]
fn no_ideas_means_no_idea_of_the_day() {
    let conn = setup();
    companion::check(&conn, at(2026, 10, 26, 9, 0), &[]).unwrap();
    assert!(texts(&conn, IDEAS).is_empty());
}

#[test]
fn a_dismissed_idea_of_the_day_does_not_return_the_same_day() {
    let conn = setup();
    companion::add_idea(&conn, "Only idea", now()).unwrap();
    companion::check(&conn, at(2026, 10, 26, 8, 0), &[]).unwrap();
    let id = store::items(&conn, IDEAS, None).unwrap()[0].id;
    store::dismiss_item(&conn, id).unwrap();
    companion::check(&conn, at(2026, 10, 26, 9, 0), &[]).unwrap();
    assert!(texts(&conn, IDEAS).is_empty());
    assert_eq!(store::unread_count(&conn, IDEAS).unwrap(), 0);
}

// --- mood and face ---

#[test]
fn the_mood_follows_the_clock_the_feed_and_the_reminders() {
    let conn = setup();
    let mood = |now| companion::describe_mood(&conn, now, Some("Sam")).unwrap();

    // Late and early hours are sleepy whatever else is going on; 05:00 is awake.
    assert_eq!(mood(at(2026, 10, 26, 23, 0)).mood, "sleepy");
    assert_eq!(mood(at(2026, 10, 26, 4, 59)).mood, "sleepy");
    let awake = mood(at(2026, 10, 26, 5, 0));
    assert_eq!(awake.mood, "happy");
    assert_eq!(awake.greeting, "Rise and shine, Sam!");
    assert_eq!(awake.line, "All caught up! I'm very proud of you.");

    // An unread idea alone is calm.
    companion::add_idea(&conn, "Learn to juggle", now()).unwrap();
    companion::check(&conn, at(2026, 10, 26, 7, 0), &[]).unwrap();
    assert_eq!(mood(now()).mood, "calm");
    assert_eq!(mood(now()).line, "Your idea of the day is waiting. No rush!");

    // Anything unread in Today, or a reminder that is due, is worried.
    reminder(&conn, "One", at(2026, 10, 26, 9, 0), Repeat::None);
    reminder(&conn, "Two", at(2026, 10, 26, 9, 30), Repeat::None);
    assert_eq!(mood(now()).mood, "worried");
    assert_eq!(mood(now()).line, "2 reminders are waiting on you. Mark them done or snooze them and I'll relax.");
    assert_eq!(mood(at(2026, 10, 27, 1, 0)).line, "I'm drowsy, but 2 reminders are still waiting for you.");
}

#[test]
fn something_unread_in_today_worries_pip_even_with_nothing_due() {
    let conn = setup();
    reminder(&conn, "Tomorrow thing", at(2026, 10, 27, 9, 0), Repeat::None);
    companion::check(&conn, now(), &[]).unwrap();
    let feeling = companion::describe_mood(&conn, now(), None).unwrap();
    assert_eq!(feeling.mood, "worried");
    assert_eq!(feeling.line, "There's a heads-up waiting in Today. Have a look when you can.");
    // Reading it makes pip happy again.
    store::mark_all_read(&conn, TODAY).unwrap();
    assert_eq!(companion::describe_mood(&conn, now(), None).unwrap().mood, "happy");
}

fn faces(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn the_face_follows_the_mood_using_the_first_face_that_exists() {
    let conn = setup();
    let all = faces(&["blank", "laughing", "worried", "yawning", "smiling"]);

    companion::check(&conn, at(2026, 10, 26, 10, 0), &all).unwrap();
    assert_eq!(expressions(&conn), "laughing"); // happy: "happy" is missing, "laughing" is next

    reminder(&conn, "Due", at(2026, 10, 26, 9, 0), Repeat::None);
    companion::check(&conn, at(2026, 10, 26, 10, 0), &all).unwrap();
    assert_eq!(expressions(&conn), "worried");

    companion::check(&conn, at(2026, 10, 26, 23, 30), &all).unwrap();
    assert_eq!(expressions(&conn), "yawning"); // sleepy: "sleepy", "sleeping", "tired" are missing
}

#[test]
fn with_no_matching_face_or_no_list_the_face_is_left_alone() {
    let conn = setup();
    reminder(&conn, "Due", at(2026, 10, 26, 9, 0), Repeat::None);
    companion::check(&conn, now(), &[]).unwrap();
    assert_eq!(expressions(&conn), "smiling");
    companion::check(&conn, now(), &faces(&["laughing"])).unwrap();
    assert_eq!(expressions(&conn), "smiling"); // worried has no match in the list
}

#[test]
fn a_check_works_even_if_the_wisp_was_never_registered() {
    let conn = schema::open_in_memory().unwrap();
    companion::check(&conn, now(), &faces(&["worried"])).unwrap();
}

// --- through the scheduler ---

#[test]
fn the_scheduler_runs_companion_and_a_check_does_its_work() {
    let conn = setup();
    reminder(&conn, "Call mum", at(2020, 1, 1, 9, 0), Repeat::None);
    let fetcher = CompanionFetcher::new().with_now(now()).with_faces(faces(&["worried"]));
    let mut scheduler = Scheduler::new(vec![Box::new(fetcher)]);
    let done = scheduler.tick(&conn, scheduler::unix_now()).unwrap();
    assert_eq!(done, [Fetch { wisp_id: "companion".into(), outcome: Outcome::Fetched }]);
    assert_eq!(texts(&conn, TODAY).len(), 1);
    assert_eq!(expressions(&conn), "worried");
    // Checked a moment ago, so the next tick leaves it alone.
    assert!(scheduler.tick(&conn, scheduler::unix_now() + 10).unwrap().is_empty());
}

// --- odd data ---

#[test]
fn a_reminder_with_an_unreadable_time_does_not_stop_the_others() {
    let conn = setup();
    reminder(&conn, "Fine", now(), Repeat::None);
    conn.execute(
        "INSERT INTO companion_notes (kind, text, due_at, repeat_rule, created_at) VALUES ('reminder', 'Broken', 'garbage', 'none', 'x')",
        [],
    )
    .unwrap();
    let broken = conn.last_insert_rowid();
    companion::check(&conn, now(), &[]).unwrap();
    assert_eq!(texts(&conn, TODAY), ["It's time: Fine"]);
    let list: Vec<_> = companion::list_reminders(&conn, now()).unwrap().into_iter().map(|r| r.text).collect();
    assert_eq!(list, ["Fine"]);
    companion::describe_mood(&conn, now(), None).unwrap();
    // Finishing it just marks it done.
    companion::complete_reminder(&conn, broken, now()).unwrap();
}