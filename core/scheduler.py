import threading
from core import db
from bots.registry import ALL_BOTS

INTERVAL_SECONDS = 45 * 60

def run_cycle(on_complete = None):
    print("Running fetch cycle...")
    for bot in ALL_BOTS:
        bot.register()
        if db.is_bot_active(bot.BOT_ID):
            bot.fetch()
            print(f"Unread for {bot.BOT_ID}:", db.get_bot_unread(bot.BOT_ID))
        else:
            print(f"{bot.BOT_ID} is inactive, skipping fetch")
    
    if on_complete:
        on_complete()
    schedule_next(on_complete)

def schedule_next(on_complete = None):
    timer = threading.Timer(INTERVAL_SECONDS, run_cycle, args = (on_complete,))
    timer.daemon = True
    timer.start()

def start(on_complete = None):
    schedule_next(on_complete)

if __name__ == "__main__":
    db.init_db()
    for bot in ALL_BOTS:
        bot.register()
    print("Scheduler started - fetching every", INTERVAL_SECONDS, "seconds. Ctrl+C to stop.")
    start()
    
    while True:
        pass
