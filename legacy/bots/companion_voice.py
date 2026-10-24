"""Everything Companion says lives in this one file, so changing its voice means editing here
and nowhere else. The voice is playful and warm."""

#Greetings, by part of the day.
GREETINGS = {
    "morning": [
        "Good morning{name}!",
        "Morning{name}! Fresh day, fresh start.",
        "Rise and shine{name}!",
    ],
    "afternoon" : [
        "Good afternoon{name}!",
        "Hey{name}, hope the day is treating you well.",
        "Afternoon{name}! Still going strong?",
    ],
    "evening" : [
        "Good evening{name}!",
        "Evening{name}! Nearly time to unwind.",
        "Hey{name}, winding down?"
    ],
    "night" : [
        "Still up{name}?",
        "It's late{name}. Don't forget to rest!",
        "Shh{name}, I'm half asleep...",
    ],
}

#what the user will be shown after saving a note, and/or when a list is empty
SAVED_REMINDER = "Got it! I'll remind you."
SAVED_IDEA = "Tucked into the jar!"
EMPTY_REMINDERS = "Nothing coming up. Enjoy the quiet!"
EMPTY_IDEAS = "The jar is empty. Drop an idea in!"

#messages and window titles
COMPANION_OFF = "I'm switched off right now. Turn me on in the Hub and I'll keep that for you."
IDEA_WINDOW = "A new idea!"
REMINDER_WINDOW = "Remind me..."
IDEA_PLACEHOLDER = "What's the idea?"
REMINDER_PLACEHOLDER = "What should I remind you about?"

#friendly alternatives for error, i am doing this so it fits the overall intended vibe
ERRORS = {
    "empty": "Oops, there's nothing written yet.",
    "too_long": "That's a bit of a mouthful, Please keep it under {limit} characters.",
    "repeat": "Pick how often it repeats.",
    "when": "Pick a date and a time, please.",
    "past": "That time has already gone by. Pick one a little later.",
    "note_gone": "Hmm, I can't find that note. Maybe it's already gone.",
    "reminder_gone": "Hmm, I can't find that reminder.",
    "snooze": "Pick how long to snooze for.",
}

def error(kind, **details):
    return ERRORS[kind].format(**details)

def _pick(options, now, salt = 0):
    """The same choice all day (so the greeting doesn't flicker on every redraw), a different one tomorrow."""
    return options[(now.toordinal() * 7 + salt) % len(options)]

def part_of_day(now, mood):
    if mood == "sleepy":
        return "night"
    if now.hour < 12:
        return "morning"
    if now.hour < 17:
        return "afternoon"
    return "evening"

def greeting(now, mood, name = None):
    part = part_of_day(now, mood)
    name_text = f", {name.strip()}" if name and name.strip() else ""
    return _pick(GREETINGS[part], now, salt = len(part)).format(name = name_text)

def _plural(count, one, many):
    return one if count == 1 else many.format(count = count)

def mood_line(mood, due, today_unread, ideas_unread):
    """One short sentence about how Companion feels, matching its face."""
    if mood == "sleepy":
        if due > 0:
            return _plural(due, "I'm drowsy, but a reminder is still waiting for you.",
                        "I'm drowsy, but {count} reminders are still waiting for you.")
        return "Zzz... all quiet. I'll keep an eye out while you rest."
    if mood == "worried":
        if due > 0:
            return _plural(due, "A reminder is waiting on you. Mark it done or snooze it and I'll relax.",
                        "{count} reminders are waiting on you. Mark them done or snooze them and I'll relax.")
        return "There's a heads-up waiting in Today. Have a look when you can."
    if mood == "happy":
        return "All caught up! I'm very proud of you."
    return "Your idea of the day is waiting. No rush!"

#items that show up on the channels
def due_item(text, was_due = None):
    if was_due:
        return f"Better late than never: {text} (was due {was_due})"
    return f"It's time: {text}"

def eve_item(text, at):
    return f"Heads-up! Tomorrow at {at}: {text}"

def idea_item(text):
    return f"Remember this one? {text}"

#the pop-up when any wisp finds something new
def new_items(name):
    return f"{name} has something new for you"