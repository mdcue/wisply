from core import db, account
from bots import deals

def main():
    acc = account.load_or_create_account()
    print(f"Welcome back, {acc['name']}!")
    
    db.init_db()
    db.add_channel("deals", "Game Deals", "Discounted games", "#4CAF50")
    db.add_channel("free_games", "Free Games", "Games that are free right now", "#2196F3")
    
    deals.fetch()
    
    print(f"You have {db.get_unread_count('deals')} unread deals.")
    print(f"You have {db.get_unread_count('free_games')} unread free games.")

if __name__ == "__main__":
    main()