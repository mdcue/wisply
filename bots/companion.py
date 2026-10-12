import argparse
import calendar
from datetime import datetime, timedelta

from core import db, appearance, icons

BOT_ID = "companion"
BOT_NAME = "Companion"
BOT_COLOR = "#ff7a59"
DESCRIPTION = "Reminds you what's coming up and brings your saved ideas back."

TODAY = "co_today"
IDEAS = "co_ideas"

TIME_FORMAT = "%Y-%m-%d %H:%M"
REPEATS = ("none", "daily", "weekly", "monthly", "yearly")
LATE_MINUTES = 60 #a reminder fired later than this says when it was really due
IDEA_HOUR = 6 #the idea of the day appears from this hour on
SLEEPY_FROM = 23 #between SLEEPY_FROM and SLEEPY_UNTIL the face looks sleepy
SLEEPY_UNTIL = 5

#For each mood, expression, names to try in order. The first one that exists in your
#expression folder is used, so a missing name never breaks anything.
MOODS = {
    "sleepy": ["sleepy", "sleeping", "tired", "yawning"],
    "worried": ["worried", "concerned", "anxious", "nervous", "sad"],
    "happy": ["happy", "laughing", "smiling"],
    "calm": ["smiling", "happy"],
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
            due_at TEXT,
            repeat_rule TEXT NOT NULL DEFAULT 'none',
            done INTEGER NOT NULL DEFAULT 0,
            last_shown TEXT,
            created_at TEXT NOT NULL
        )"""
    )

def register():
    _ensure_table()
    is_new = db.get_bot(BOT_ID) is None
    db.add_bot(BOT_ID, BOT_NAME, BOT_COLOR)
    if is_new:
        appearance.set_bot_appearance(BOT_ID, color = BOT_COLOR, expression = "smiling")
    db.add_channel(TODAY, "Today", "Reminders that are due, and what is coming up", "#ff7a59", bot_id = BOT_ID, position = 0)
    db.add_channel(IDEAS, "Idea of the day", "One of your saved ideas, picked each morning", "#f5c542", bot_id = BOT_ID, position = 1)

# storing notes

def add_reminder(text, due, repeat = "none"):
    _connect_and_run(
        "INSERT INTO companion_notes (kind, text, due_at, repeat_rule, created_at) VALUES ('reminder', ?, ?, ?, ?)",
        (text.strip(), due.strftime(TIME_FORMAT),  repeat, datetime.now().strftime(TIME_FORMAT)),
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

#dates

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

#making the items

def is_new_item(key):
    conn = db.get_connection()
    cur = conn.execute("SELECT COUNT(*) FROM items WHERE timestamp = ?", (key,))
    exists = cur.fetchone()[0] > 0
    conn.close()
    return not exists

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
            if is_new_item(key):
                db.add_item(TODAY, f"Tomorrow at {due:%H:%M}: {text}", key, url = None, price = None, discount = None)
        
        if due > now:
            continue
        key = f"rem:{note_id}:{due_at}"
        if is_new_item(key):
            late = now - due > timedelta(minutes = LATE_MINUTES)
            when = f" (was due {due:%a %d %b, %H:%M})" if late else ""
            db.add_item(TODAY, f"Due: {text}{when}", key, url = None, price = None, discount = None)
        
        if rule in REPEATS and rule != "none":
            _connect_and_run(
                "UPDATE companion_notes SET due_at = ? WHERE id = ?",
                (_next_due(due, rule, now).strftime(TIME_FORMAT), note_id),
            )
        else:
            _connect_and_run("UPDATE companion_notes SET done = 1 WHERE id = ?", (note_id,))

def _surface_idea(now):
    if now.hour < IDEA_HOUR:
        return
    key = f"idea:{now:%Y-%m-%d}"
    if not is_new_item(key):
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

#the face changes

def _mood(now):
    if now.hour >= SLEEPY_FROM or now.hour < SLEEPY_UNTIL:
        return "sleepy"
    today_unread = db.get_unread_count(TODAY)
    if today_unread > 0:
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
    if expression and expression != bot[5]: #bot[5] is the current held expression; only redraw the icon when it changes
        appearance.set_bot_appearance(BOT_ID, expression = expression)

def fetch(now = None):
    now = now or datetime.now()
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
            print(f"{note_id:>3} {kind:<8}{'  [done]' if done else ''}{extra} {text}")

if __name__ == "__main__":
    _cli()
