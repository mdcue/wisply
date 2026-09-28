import requests
from core import db

HEADERS = {"User-Agent": "wisply/1.0 (contact@example.com)"}
BOT_ID = "game_deals"

def register():
    db.add_bot(BOT_ID, "Game Deals", "#4CAF50")
    db.add_channel("deals", "Deals", "Games on sale", "#4CAF50", bot_id = BOT_ID)
    db.add_channel("free_games", "Free games", "Games that are free right now", "#2196F3", bot_id = BOT_ID)

def get_store_names():
    response = requests.get("https://www.cheapshark.com/api/1.0/stores", headers = HEADERS)
    response.raise_for_status()
    stores = response.json()
    return {
        store["storeID"]: store["storeName"] for store in stores if store.get("isActive")
    }

def is_new_deal(deal_id):
    conn = db.get_connection()
    cur = conn.execute(
        "SELECT COUNT(*) FROM items WHERE timestamp = ?",(deal_id,)
    )
    exists = cur.fetchone()[0] > 0
    conn.close()
    return not exists

def fetch_deals(store_names):
    response = requests.get(
        "https://www.cheapshark.com/api/1.0/deals",
        params = {
            "upperPrice": 60,
        }, headers = HEADERS)
    response.raise_for_status()
    deals = response.json()
    
    for deal in deals[:10]:
        deal_id = deal.get("dealID", "")
        if is_new_deal(deal_id):
            title = deal.get("title")
            price = deal.get("salePrice")
            normal = deal.get("normalPrice")
            savings = round(float(deal.get("savings") or 0))
            store = store_names.get(deal.get("storeID"), "Unknown store")
            text = f"{title} - ${price} (was ${normal}, {savings}% off) ({store})"
            db.add_item("deals", text, deal_id)

def fetch_free_games(store_names):
    response = requests.get(
        "https://www.cheapshark.com/api/1.0/deals",
        params = {
            "upperPrice": 0,
        }, headers = HEADERS)
    response.raise_for_status()
    deals = response.json()
    
    for deal in deals[:10]:
        deal_id = deal.get("dealID", "")
        if is_new_deal(deal_id):
            title = deal.get("title")
            normal = deal.get("normalPrice")
            store = store_names.get(deal.get("storeID"), "Unknown store")
            text = f"{title} is free right now! (normally ${normal}) ({store})"
            db.add_item("free_games", text, deal_id)

def fetch():
    store_names = get_store_names()
    fetch_deals(store_names)
    fetch_free_games(store_names)

if __name__ == "__main__":
    db.init_db()
    register()
    
    fetch()
    
    print("Unread deals", db.get_unread_count("deals"))
    print("Unread free games:", db.get_unread_count("free_games"))
