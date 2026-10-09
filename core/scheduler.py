import threading
import time
import traceback

from core import db
from bots.registry import ALL_BOTS

TICK_SECONDS = 60
RETRY_AFTER_FAILURE_SECONDS = 300

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

def run_tick(on_complete = None):
    try:
        for bot in ALL_BOTS:
            try:
                bot.register()
                if not db.is_bot_active(bot.BOT_ID):
                    continue
                if time.time() < _retry_not_before.get(bot.BOT_ID, 0):
                    continue
                if not _is_due(bot.BOT_ID):
                    continue
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

def _is_due(bot_id):
    bot = db.get_bot(bot_id)
    if not bot:
        return False
    interval = bot[6]
    last_fetched_at = bot[7]
    
    if interval == 0:
        return False
    
    if last_fetched_at is None:
        return True
    
    conn = db.get_connection()
    row = conn.execute(
        "SELECT (strftime('%s', 'now') - strftime('%s', ?)) >= ?",
        (last_fetched_at, interval)
    ).fetchone()
    conn.close()
    return bool(row[0])

def fetch_startup_only_bots():
    """Called once at app launch for wisps set to 'only on startup'."""
    for bot in ALL_BOTS:
        try:
            bot.register()
            if db.is_bot_active(bot.BOT_ID):
                bot_row = db.get_bot(bot.BOT_ID)
                if bot_row and bot_row[6] == 0:
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