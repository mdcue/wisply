import argparse
import calendar
import threading
from datetime import datetime, timedelta

from core import db, appearance, icons
from bots.common import register_wisp

BOT_ID = "companion"
BOT_NAME = "pip"
BOT_COLOR = "#ff7a59"
DESCRIPTION = "Reminds you what's coming up and brings your saved ideas back."

TODAY = "co_today"
IDEAS = "co_ideas"

TIME_FORMAT = "%Y-%m-%d %H:%M"
REPEATS = ("none", "daily", "weekly", "monthly", "yearly")
LATE_MINUTES = 60 #a reminder fired later than this says when it was really due
IDEA_HOUR = 6 #for idea of the day that will appears from this hour on
SLEEPY_FROM = 23 #the time between SLEEPY_FROM and SLEEPY_UNTIL the face looks sleepy
SLEEPY_UNTIL = 5
MAX_TEXT = 300 #longest note the Hub will accept
TOMORROW_HOUR = 9 #"snooze until tomorrow" means tomorrow at this hour
FETCH_INTERVALS = (60, 300, 900, 1800, 3600) #Companion only reads your own notes, so it can check every minute

#For each mood, expression, names to try in order. The first one that exists in your
#expression folder is used, so a missing name never breaks anything
MOODS = {
    "sleepy": ["sleepy", "sleeping", "tired", "yawning", "blank"],
    "worried": ["worried", "concerned", "anxious", "nervous"],
    "happy": ["happy", "laughing", "smiling"],
    "calm": ["content", "smiling", "happy"],
}

def _connect_and_run(sql, params = (), fetch = False):
    conn = db.get_connection()
    try:
        cur = conn.execute(sql, params)
        rows = cur.fetchall() if fetch else None
        conn.commit()
        return rows
    finally:
        conn.close()

def _ensure_table():
    _connect_and_run(
    """CREATE TABLE IF NOT EXISTS companion_notes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        kind TEXT NOT NULL,
        text TEXT NOT NULL,
        due_at NULL,
        repeat_rule TEXT NOT NULL DEFAULT 'none',
        done INTEGER NOT NULL DEFAULT 0,
        last_shown TEXT,
        created_at TEXT NOT NULL
    )"""
)

#(channel id, title, description, colour)
CHANNELS = [
    (TODAY, "Today", "Reminders that are due, and what is coming up", "#ff7a59"),
    (IDEAS, "Idea of the day", "One of your saved ideas, picked each morning", "#f5c542")
]

def register():
    _ensure_table()
    register_wisp(BOT_ID, BOT_NAME, BOT_COLOR, CHANNELS, fetch_interval = 60)

#this is for storing notes
def add_reminder(text, due, repeat = "none"):
    _connect_and_run(
        "INSERT INTO companion_notes (kind, text, due_at, repeat_rule, created_at) VALUES ('reminder', ?, ?, ?, ?)",
        (text.strip(), due.strftime(TIME_FORMAT), repeat, datetime.now().strftime(TIME_FORMAT)),
    )

def add_idea(text):
    _connect_and_run(
        "INSERT INTO companion_notes (kind, text, created_at) VALUES ('idea', ?, ?)",
        (text.strip(), datetime.now().strftime(TIME_FORMAT)),
    )

def delete_note(note_id):
    _connect_and_run("DELETE FROM companion_notes WHERE id = ?", (note_id,))

def list_notes():
    return _connect_and_run(
        "SELECT id, kind, text, due_at, repeat_rule, done FROM companion_notes ORDER BY kind, due_at, id",
        fetch = True,
    )

#for dates
def _add_months(moment, months):
    index = moment.month - 1 + months
    year = moment.year + index // 12
    month = index % 12 + 1
    day = min(moment.day, calendar.monthrange(year, month)[1])
    return moment.replace(year = year, month = month, day = day)

def _next_due(due, rule, now):
    """The first repeat of `due` that is still in the future, so a reminder that was
    missed for days fires once and moves on instead of firing for every missed day."""
    while due <= now:
        if rule == "daily":
            due += timedelta(days = 1)
        elif rule == "weekly":
            due += timedelta(weeks = 1)
        elif rule == "monthly":
            due = _add_months(due, 1)
        elif rule == "yearly":
            due = _add_months(due, 12)
        else:
            break
    return due

#notes, as the Hub shows and changes them

def _clean_text(text):
    text = (text or "").strip()
    if not text:
        raise ValueError("Write something first.")
    if len(text) > MAX_TEXT:
        raise ValueError(f"That's a bit long. Keep it under {MAX_TEXT} characters.")
    return text

def _clean_repeat(repeat):
    repeat = (repeat or "none").strip() or "none"
    if repeat not in REPEATS:
        raise ValueError("Pick how often it repeats.")
    return repeat

def _parse_when(date_text, time_text):
    try:
        return datetime.strptime(f"{date_text} {time_text}", TIME_FORMAT)
    except (ValueError, TypeError):
        raise ValueError("Pick a date and a time.") from None

def _check_not_past(due, now):
    if due < now - timedelta(minutes = 1):
        raise ValueError("That time has already passed.")

def _day_label(due, now):
    days = (due.date() - now.date()).days
    if days == 0:
        return f"Today {due:%H:%M}"
    if days == 1:
        return f"Tomorrow {due:%H:%M}"
    if days == -1:
        return f"Yesterday {due:%H:%M}"
    return f"{due:%a %d %b}, {due:%H:%M}"

def list_reminders(now = None):
    """Reminders that are not finished, soonest first. `status` is "due" once their time has
    come (they stay until you mark them done or snooze them), otherwise "upcoming"."""
    now = now or datetime.now()
    rows = _connect_and_run(
        "SELECT id, text, due_at, repeat_rule FROM companion_notes "
        "WHERE kind = 'reminder' AND done = 0 AND due_at IS NOT NULL ORDER BY due_at, id",
        fetch = True,
    )
    reminders = []
    for note_id, text, due_at, rule in rows:
        due = datetime.strptime(due_at, TIME_FORMAT)
        reminders.append({
            "id": note_id,
            "text": text,
            "date": f"{due:%Y-%m-%d}",
            "time": f"{due:%H:%M}",
            "label": _day_label(due, now),
            "repeat": rule,
            "status": "due" if due <= now else "upcoming",
        })
    return reminders

def list_ideas():
    rows = _connect_and_run("SELECT id, text FROM companion_notes WHERE kind = 'idea' ORDER BY id DESC", fetch = True)
    return [{"id": note_id, "text": text} for note_id, text in rows]

def create_reminder(text, date_text, time_text, repeat = "none", now = None):
    now = now or datetime.now()
    text = _clean_text(text)
    due = _parse_when(date_text, time_text)
    _check_not_past(due, now)
    add_reminder(text, due, _clean_repeat(repeat))

def create_idea(text):
    add_idea(_clean_text(text))

def update_note(note_id, text, date_text = None, time_text = None, repeat = None, now = None):
    now= now or datetime.now()
    rows = _connect_and_run("SELECT kind, due_at FROM companion_notes WHERE id = ?", (note_id,), fetch = True)
    if not rows:
        raise ValueError("That note is gone.")
    kind, stored_due = rows[0]
    text = _clean_text(text)
    if kind == "idea":
        _connect_and_run("UPDATE companion_notes SET text = ? WHERE id = ?", (text, note_id))
        return
    due = _parse_when(date_text, time_text)
    if due.strftime(TIME_FORMAT) != stored_due:
        _check_not_past(due, now)
    _connect_and_run(
        "UPDATE companion_notes SET text = ?, due_at = ?, repeat_rule = ?, done = 0 WHERE id = ?",
        (text, due.strftime(TIME_FORMAT), _clean_repeat(repeat), note_id),
    )

def complete_reminder(note_id, now = None):
    """One-off reminders are finished. A repeating one moves on to its next time, so marking
    it done skips this round."""
    now = now or datetime.now()
    rows = _connect_and_run(
        "SELECT due_at, repeat_rule FROM companion_notes WHERE id = ? AND kind = 'reminder'", (note_id,), fetch = True
    )
    if not rows:
        return
    due_at, rule = rows[0]
    if rule == "none":
        _connect_and_run("UPDATE companion_notes SET done = 1 WHERE id = ?", (note_id,))
        return
    due = datetime.strptime(due_at, TIME_FORMAT)
    following = _next_due(due, rule, max(now, due))
    _connect_and_run("UPDATE companion_notes SET due_at = ? WHERE id = ?", (following.strftime(TIME_FORMAT), note_id))

def _snooze_time(option, now):
    if option == "10m":
        later = now + timedelta(minutes = 10)
    elif option == "1h":
        later = now + timedelta(hours = 1)
    elif option == "tomorrow":
        later = (now + timedelta(days = 1)).replace(hour = TOMORROW_HOUR, minute = 0)
    else:
        raise ValueError("Pick how long to snooze.")
    return later.replace(second = 0, microsecond = 0)

def snooze_reminder(note_id, option, now = None):
    """Moves a reminder to later. A repeating reminder keeps its own schedule: the snooze is a
    one-off copy, and the original moves on as if it had been done."""
    now = now or datetime.now()
    later = _snooze_time(option, now)
    rows = _connect_and_run(
        "SELECT text, repeat_rule FROM companion_notes WHERE id = ? AND kind = 'reminder'", (note_id,), fetch = True
    )
    if not rows:
        raise ValueError("That reminder is gone.")
    text, rule = rows[0]
    if rule == "none":
        _connect_and_run(
            "UPDATE companion_notes SET due_at = ?, done = 0 WHERE id = ?", (later.strftime(TIME_FORMAT), note_id)
        )
        return
    add_reminder(text, later, "none")
    complete_reminder(note_id, now)

#making the items
def _fire_reminders(now):
    rows = _connect_and_run(
        "SELECT id, text, due_at, repeat_rule FROM companion_notes "
        "WHERE kind = 'reminder' AND done = 0 AND due_at IS NOT NULL",
        fetch = True,
    )
    tomorrow = (now + timedelta(days = 1)).date()
    for note_id, text, due_at, rule in rows:
        due = datetime.strptime(due_at, TIME_FORMAT)
        if due.date() == tomorrow:
            key = f"rem:{note_id}:{due_at}:eve"
            if not db.has_item(key):
                db.add_item(TODAY, f"Tomorrow at {due:%H:%M}: {text}", key, url = None, price = None, discount = None)
        
        if due > now:
            continue
        key = f"rem:{note_id}:{due_at}"
        if not db.has_item(key):
            late = now - due > timedelta(minutes = LATE_MINUTES)
            when = f" (was due {due:%a %d %b, %H:%M})" if late else ""
            db.add_item(TODAY, f"Due: {text}{when}", key, url = None, price = None, discount = None)
        
        if rule in REPEATS and rule != "none":
            _connect_and_run(
                "UPDATE companion_notes SET due_at = ? WHERE id = ?",
                (_next_due(due, rule, now).strftime(TIME_FORMAT), note_id),
            )

def _surface_idea(now):
    if now.hour < IDEA_HOUR:
        return
    key = f"idea:{now:%Y-%m-%d}"
    if db.has_item(key):
        return
    rows = _connect_and_run(
        "SELECT id, text FROM companion_notes WHERE kind = 'idea' "
        "ORDER BY last_shown IS NOT NULL, last_shown, RANDOM() LIMIT 1",
        fetch = True,
    )
    if not rows:
        return
    note_id, text = rows[0]
    db.add_item(IDEAS, f"Idea: {text}", key, url = None, price = None, discount = None)
    _connect_and_run(
        "UPDATE companion_notes SET last_shown = ? WHERE id = ?",
        (now.strftime(TIME_FORMAT), note_id),
    )

#the dynamic face changes
def _count_due(now):
    rows = _connect_and_run(
        "SELECT COUNT(*) FROM companion_notes WHERE kind = 'reminder' AND done = 0 AND due_at <= ?",
        (now.strftime(TIME_FORMAT),), fetch = True,
    )
    return rows[0][0]

def _mood(now):
    if now.hour >= SLEEPY_FROM or now.hour < SLEEPY_UNTIL:
        return "sleepy"
    today_unread = db.get_unread_count(TODAY)
    if today_unread > 0 or _count_due(now) > 0:
        return "worried"
    if today_unread + db.get_unread_count(IDEAS) == 0:
        return "happy"
    return "calm"

def _pick_expression(candidates):
    available = set(icons.get_available_expressions())
    return next((name for name in candidates if name in available), None)

def _update_face(now):
    bot = db.get_bot(BOT_ID)
    if not bot:
        return
    expression = _pick_expression(MOODS[_mood(now)])
    if expression and expression != bot[5]: #bot[5] is the index holding the current expression
        appearance.set_bot_appearance(BOT_ID, expression = expression)

_fetch_lock = threading.Lock() #the scheduler and the Hub can both ask for a check at once

def fetch(now = None):
    now = now or datetime.now()
    with _fetch_lock:
        _fire_reminders(now)
        _surface_idea(now)
        _update_face(now)

#temp command line, until the Hub has a proper input
def _parse_due(text):
    for pattern in (TIME_FORMAT, "%Y-%m-%d"):
        try:
            parsed = datetime.strptime(text, pattern)
        except ValueError:
            continue
        return parsed.replace(hour = 9, minute = 0) if pattern == "%Y-%m-%d" else parsed
    raise argparse.ArgumentTypeError('use "YYYY-MM-DD HH:MM" or "YYYY-MM-DD" (which means 09:00)')

def _cli():
    parser = argparse.ArgumentParser(prog = "python -m bots.companion", description = "Developer tools for Companion")
    sub = parser.add_subparsers(dest = "command", required = True)
    add = sub.add_parser("add", help = "add a reminder")
    add.add_argument("text")
    add.add_argument("--due", required = True, type = _parse_due)
    add.add_argument("--repeat", choices = REPEATS, default = "none")
    idea = sub.add_parser("idea", help = "drop an idea in the jar")
    idea.add_argument("text")
    sub.add_parser("list", help = "show every saved note")
    delete = sub.add_parser("delete", help = "delete a note by its id")
    delete.add_argument("note_id", type = int)
    sub.add_parser("check", help = "run one check right now")
    args = parser.parse_args()
    
    db.init_db()
    register()
    if args.command == "add":
        add_reminder(args.text, args.due, args.repeat)
        print("Reminder added.")
    elif args.command == "idea":
        add_idea(args.text)
        print("Idea saved.")
    elif args.command == "delete":
        delete_note(args.note_id)
        print("Deleted.")
    elif args.command == "check":
        fetch()
        print("Unread today:", db.get_unread_count(TODAY), "| ideas:", db.get_unread_count(IDEAS))
    else:
        for note_id, kind, text, due_at, rule, done in list_notes():
            extra = f" due {due_at}" + (f" ({rule})" if rule != "none" else "") if due_at else ""
            print(f"{note_id:>3}  {kind:<8}{'   [done]' if done else ''}{extra} {text}")

if __name__ == "__main__":
    _cli()