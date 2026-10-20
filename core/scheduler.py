import threading
import time
import traceback
from datetime import datetime, timezone

from core import db
from bots.registry import ALL_BOTS

TICK_SECONDS = 60
RETRY_AFTER_FAILURE_SECONDS = 300
DUE_TOLERANCE_SECONDS = 5 #a wisp counts as due a few seconds early, so a 1-minutes interval is not missed by a hair
QUITE_BELOW_SECONDS = 300 #wisps checked more often than this don't write a log line for every check

_retry_not_before = {}

def _fetch_safely(bot):
    """Runs one wisp's fetch. A failure is logged and put on a short cooldown instead of
    crashing the scheduler or hammering the source every tick. Returns True on success."""
    try:
        bot.fetch()
    except Exception:
        print(f"[{bot.BOT_ID}] fetch failed, will retry in {RETRY_AFTER_FAILURE_SECONDS // 60} minutes:")
        traceback.print_exc()
        _retry_not_before[bot.BOT_ID] = time.time() + RETRY_AFTER_FAILURE_SECONDS
        return False
    _retry_not_before.pop(bot.BOT_ID, None)
    db.mark_fetched(bot.BOT_ID)
    return True

def _seconds_since(utc_text):
    """Seconds since a time saved by SQLite's datetime('now'), which is UTC."""
    then = datetime.strptime(utc_text, "%Y-%m-%d %H:%M:%S").replace(tzinfo = timezone.utc)
    return (datetime.now(timezone.utc) - then).total_seconds()

def _is_due(bot_row):
    """True once a wisp's check interval has passed. Wisps set to 'only on startup' (0) are never due here."""
    if bot_row.fetch_interval == 0:
        return False
    if bot_row.last_fetched is None:
        return True
    try:
        return _seconds_since(bot_row.last_fetched) >= bot_row.fetch_interval - DUE_TOLERANCE_SECONDS
    except ValueError:
        return True

def run_tick(on_complete = None):
    try:
        for bot in ALL_BOTS:
            try:
                bot_row = db.get_bot(bot.BOT_ID)
                if not bot_row or not bot_row.active:
                    continue
                if time.time() < _retry_not_before.get(bot.BOT_ID, 0):
                    continue
                if not _is_due(bot_row):
                    continue
                chatty = bot_row.fetch_interval >= QUITE_BELOW_SECONDS
                if chatty:
                    print(f"Fetching {bot.BOT_ID}...")
                if _fetch_safely(bot):
                    print(f"Unread for {bot.BOT_ID}:", db.get_bot_unread(bot.BOT_ID))
            except Exception:
                traceback.print_exc()
                
        if on_complete:
            on_complete()
    except Exception:
        traceback.print_exc()
    finally:
        schedule_next(on_complete)

def fetch_startup_only_bots():
    """Called once at the app launch for wisps set to 'only on startup'."""
    for bot in ALL_BOTS:
        try:
            bot_row = db.get_bot(bot.BOT_ID)
            if bot_row and bot_row.active and bot_row.fetch_interval == 0:
                print(f"Startup-only fetch for {bot.BOT_ID}...")
                _fetch_safely(bot)
        except Exception:
            traceback.print_exc()

def schedule_next(on_complete = None):
    timer = threading.Timer(TICK_SECONDS, run_tick, args = (on_complete,))
    timer.daemon = True
    timer.start()

def start(on_complete = None):
    schedule_next(on_complete)

if __name__ == "__main__":
    db.init_db()
    for bot in ALL_BOTS:
        bot.register()
    
    print("Scheduler started - checking every", TICK_SECONDS, "seconds. (Ctrl+C to stop)")
    start()
    
    while True:
        time.sleep(1)
