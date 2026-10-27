//! Everything Companion ("pip") says lives in this one file, so changing its voice means editing
//! here and nowhere else. The voice is playful and warm. The words are copied from the Python
//! app's `companion_voice.py` exactly.

use chrono::{Datelike, NaiveDateTime, Timelike};

/// How Companion feels. It decides the face and the line at the top of its panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Sleepy,
    Worried,
    Happy,
    Calm,
}

impl Mood {
    pub fn name(self) -> &'static str {
        match self {
            Mood::Sleepy => "sleepy",
            Mood::Worried => "worried",
            Mood::Happy => "happy",
            Mood::Calm => "calm",
        }
    }

    /// Face names to try, in order. The first one that exists is used, so a missing face never
    /// breaks anything.
    pub fn expressions(self) -> &'static [&'static str] {
        match self {
            Mood::Sleepy => &["sleepy", "sleeping", "tired", "yawning", "blank"],
            Mood::Worried => &["worried", "concerned", "anxious", "nervous"],
            Mood::Happy => &["happy", "laughing", "smiling"],
            Mood::Calm => &["content", "smiling", "happy"],
        }
    }
}

// Greetings, by part of the day. `{name}` becomes ", Sam" when the name is known, or nothing.
const MORNING: &[&str] = &["Good morning{name}!", "Morning{name}! Fresh day, fresh start.", "Rise and shine{name}!"];
const AFTERNOON: &[&str] = &[
    "Good afternoon{name}!",
    "Hey{name}, hope the day is treating you well.",
    "Afternoon{name}! Still going strong?",
];
const EVENING: &[&str] = &["Good evening{name}!", "Evening{name}! Nearly time to unwind.", "Hey{name}, winding down?"];
const NIGHT: &[&str] = &["Still up{name}?", "It's late{name}. Don't forget to rest!", "Shh{name}, I'm half asleep..."];

// What the user sees after saving a note, and when a list is empty.
pub const SAVED_REMINDER: &str = "Got it! I'll remind you.";
pub const SAVED_IDEA: &str = "Tucked into the jar!";
pub const EMPTY_REMINDERS: &str = "Nothing coming up. Enjoy the quiet!";
pub const EMPTY_IDEAS: &str = "The jar is empty. Drop an idea in!";

// Messages and window titles from the tray.
pub const COMPANION_OFF: &str = "I'm switched off right now. Turn me on in the Hub and I'll keep that for you.";
pub const IDEA_WINDOW: &str = "A new idea!";
pub const REMINDER_WINDOW: &str = "Remind me...";
pub const IDEA_PLACEHOLDER: &str = "What's the idea?";
pub const REMINDER_PLACEHOLDER: &str = "What should I remind you about?";

/// The things that can go wrong with a note, each with a friendly sentence (see `error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    Empty,
    TooLong { limit: usize },
    Repeat,
    When,
    Past,
    NoteGone,
    ReminderGone,
    Snooze,
}

pub fn error(problem: Problem) -> String {
    match problem {
        Problem::Empty => "Oops, there's nothing written yet.".to_string(),
        Problem::TooLong { limit } => format!("That's a bit of a mouthful! Keep it under {limit} characters."),
        Problem::Repeat => "Pick how often it repeats.".to_string(),
        Problem::When => "Pick a date and a time, please.".to_string(),
        Problem::Past => "That time has already gone by. Pick one a little later.".to_string(),
        Problem::NoteGone => "Hmm, I can't find that note. Maybe it's already gone.".to_string(),
        Problem::ReminderGone => "Hmm, I can't find that reminder.".to_string(),
        Problem::Snooze => "Pick how long to snooze for.".to_string(),
    }
}

/// The same choice all day (so the greeting doesn't flicker on every redraw), a different one
/// tomorrow. `num_days_from_ce` counts days the same way Python's `toordinal` does.
fn pick<'a>(options: &[&'a str], now: NaiveDateTime, salt: usize) -> &'a str {
    let day = now.date().num_days_from_ce() as usize;
    options[(day * 7 + salt) % options.len()]
}

pub fn part_of_day(now: NaiveDateTime, mood: Mood) -> &'static str {
    if mood == Mood::Sleepy {
        "night"
    } else if now.hour() < 12 {
        "morning"
    } else if now.hour() < 17 {
        "afternoon"
    } else {
        "evening"
    }
}

pub fn greeting(now: NaiveDateTime, mood: Mood, name: Option<&str>) -> String {
    let part = part_of_day(now, mood);
    let options = match part {
        "morning" => MORNING,
        "afternoon" => AFTERNOON,
        "evening" => EVENING,
        _ => NIGHT,
    };
    let name_text = match name.map(str::trim) {
        Some(name) if !name.is_empty() => format!(", {name}"),
        _ => String::new(),
    };
    pick(options, now, part.len()).replace("{name}", &name_text)
}

fn plural(count: i64, one: &str, many: &str) -> String {
    if count == 1 {
        one.to_string()
    } else {
        many.replace("{count}", &count.to_string())
    }
}

/// One short sentence about how Companion feels, matching its face.
pub fn mood_line(mood: Mood, due: i64, _today_unread: i64, _ideas_unread: i64) -> String {
    match mood {
        Mood::Sleepy if due > 0 => plural(
            due,
            "I'm drowsy, but a reminder is still waiting for you.",
            "I'm drowsy, but {count} reminders are still waiting for you.",
        ),
        Mood::Sleepy => "Zzz... all quiet. I'll keep an eye out while you rest.".to_string(),
        Mood::Worried if due > 0 => plural(
            due,
            "A reminder is waiting on you. Mark it done or snooze it and I'll relax.",
            "{count} reminders are waiting on you. Mark them done or snooze them and I'll relax.",
        ),
        Mood::Worried => "There's a heads-up waiting in Today. Have a look when you can.".to_string(),
        Mood::Happy => "All caught up! I'm very proud of you.".to_string(),
        Mood::Calm => "Your idea of the day is waiting. No rush!".to_string(),
    }
}

// Items that show up in the channels.
pub fn due_item(text: &str, was_due: Option<&str>) -> String {
    match was_due {
        Some(when) => format!("Better late than never: {text} (was due {when})"),
        None => format!("It's time: {text}"),
    }
}

pub fn eve_item(text: &str, at: &str) -> String {
    format!("Heads-up! Tomorrow at {at}: {text}")
}

pub fn idea_item(text: &str) -> String {
    format!("Remember this one? {text}")
}

/// The pop-up when any wisp finds something new.
pub fn new_items(name: &str) -> String {
    format!("{name} has something new for you")
}