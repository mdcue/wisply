import requests

from core import db
from bots.common import USER_AGENT, register_wisp

HEADERS = {"User-Agent": USER_AGENT}
BOT_ID = "game_deals"
BOT_NAME = "Game Deals"
BOT_COLOR = "#4CAF50"
DESCRIPTION = "Tracks game deals and free games across every major store."

#(channel_id, title, description, colour)
CHANNELS = [
    ("free_games", "Free games", "Games that are free right now", "#5df300"),
    ("deals", "Deals", "Games on sale", "#2196F3"),
]

def register():
    register_wisp(BOT_ID, BOT_NAME, BOT_COLOR, CHANNELS)

def get_store_names():
    response = requests.get("https://www.cheapshark.com/api/1.0/stores", headers = HEADERS, timeout = 15)
    response.raise_for_status()
    stores = response.json()
    return {
        store["storeID"]: store["storeName"] for store in stores if store.get("isActive")
    }

def fetch_deals(store_name):
    response = requests.get(
        "https://www.cheapshark.com/api/1.0/deals",
        params = {
            "upperPrice": 60,
        }, headers = HEADERS,
        timeout = 15)
    response.raise_for_status()
    deals = response.json()
    
    for deal in reversed(deals[:10]): #the Hub lists newest first, so the best deal goes in last
        deal_id = deal.get("dealID", "")
        if not db.has_item(deal_id):
            title = deal.get("title")
            price = deal.get("salePrice")
            price_val = float(price) if price not in (None, "") else None
            if price_val is not None and price_val <= 0:
                continue
            normal = deal.get("normalPrice")
            savings = round(float(deal.get("savings") or 0))
            store = store_name.get(deal.get("storeID"), "Unknown store")
            text = f"{title} - ${price} (was ${normal}, {savings}$ off) ({store})"
            url = f"https://www.cheapshark.com/redirect?dealID={deal_id}"
            db.add_item("deals", text, deal_id, url = url, price = price_val, discount = savings)

def fetch_free_games(store_names):
    response = requests.get(
        "https://www.cheapshark.com/api/1.0/deals",
        params = {
            "upperPrice": 0,
        }, headers = HEADERS,
        timeout = 15)
    response.raise_for_status()
    deals = response.json()
    
    for deal in reversed(deals[:10]):
        deal_id = deal.get("dealID", "")
        if not db.has_item(deal_id):
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
    print("Unread free games", db.get_unread_count("free_games"))
