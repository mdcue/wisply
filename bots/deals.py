import requests
from pathlib import Path
from core import db, appearance

HEADERS = {"User-Agent": "wisply/1.0 (contact@example.com)"}
BOT_ID = "game_deals"
DESCRIPTION = "Tracks game deals and free games accross every major store."
ICON_PATH = Path(__file__).parent.parent / "ui" / "assets" / "bots" / "game_deals" / "icon.png"

def register():
    is_new = db.get_bot(BOT_ID) is None
    db.add_bot(BOT_ID, "Game Deals", "#4CAF50")
    if is_new:
        appearance.set_bot_appearance(BOT_ID, color = "#4CAF50", expression = "smiling")
    db.add_channel("free_games", "Free games", "Games that are free right now", "#5df300", bot_id = BOT_ID, position = 0)
    db.add_channel("deals", "Deals", "Games on sale", "#2196F3", bot_id = BOT_ID, position = 1)

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
            price_val = float(price) if price not in (None, "") else None
            if price_val is not None and price_val <= 0:
                continue
            normal = deal.get("normalPrice")
            savings = round(float(deal.get("savings") or 0))
            store = store_names.get(deal.get("storeID"), "Unknown store")
            text = f"{title} - ${price} (was ${normal}, {savings}% off) ({store})"
            url = f"https://www.cheapshark.com/redirect?dealID={deal_id}"
            price_val = float(price) if price not in (None, "") else None
            db.add_item("deals", text, deal_id, url = url, price = price_val, discount = savings)

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
            url = f"https://www.cheapshark.com/redirect?dealID={deal_id}"
            db.add_item("free_games", text, deal_id, url = url, price = 0.0, discount = 100)

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
