import threading
from core import db
from bots import deals

INTERVAL_SECONDS = 60 * 60

def run_cycle(on_complete = None):
    print("Running fetch cycle...")
    deals.fetch()
    print("Unread deals:", db.get_unread_count("deals"))
    print("Unread free games:", db.get_unread_count("free_games"))
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
    db.add_channel("deals", "Game Deals", "Discounted games", "#4CAF50")
    db.add_channel("free_games", "Free Games", "Games that are free right now", "#2196F3")
    
    print("Scheduler started - fetching every", INTERVAL_SECONDS, "seconds. Ctrl+C to stop.")
    start()
    
    while True:
        pass
