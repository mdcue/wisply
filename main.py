from core import db, account
from bots import deals

def main():
    acc = account.load_or_create_account()
    print(f"Welcome back, {acc['name']}!")
    
    db.init_db()
    deals.register()
    deals.fetch()
    
    print(f"You have {db.get_unread_count('deals')} unread deals.")
    print(f"You have {db.get_unread_count('free_games')} unread free games.")

if __name__ == "__main__":
    main()