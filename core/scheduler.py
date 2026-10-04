import threading
from core import db
from bots.registry import ALL_BOTS

TICK_SECONDS = 60

def run_tick(on_complete = None):
    for bot in ALL_BOTS:
        bot.register()
        if db.is_bot_active(bot.BOT_ID) and _is_due(bot.BOT_ID):
            print(f"Fetching {bot.BOT_ID}...")
            bot.fetch()
            db.mark_fetched(bot.BOT_ID)
            print(f"Unread for {bot.BOT_ID}:", db.get_bot_unread(bot.BOT_ID))
    
    if on_complete:
        on_complete()
    schedule_next(on_complete)

def _is_due(bot_id):
    bot = db.get_bot(bot_id)
    if not bot:
        return False
    interval = bot[6]
    last_fetched_at = bot[7]
    
    if last_fetched_at is None:
        return True
    
    conn = db.get_connection()
    row = conn.execute(
        "SELECT (strftime('%s', 'now') - strftime('%s', ?)) >= ?",
        (last_fetched_at, interval)
    ).fetchone()
    conn.close()
    return bool(row[0])

def schedule_next(on_complete = None):
    timer = threading.Timer(TICK_SECONDS, run_tick, args=(on_complete,))
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
        pass