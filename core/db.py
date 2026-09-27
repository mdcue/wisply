import sqlite3
from pathlib import Path

DB_PATH = Path(__file__).parent.parent / "data.db"
#__file__ is used so it can hold the path to the current file.

def get_connection():
    return sqlite3.connect(DB_PATH)

def init_db():
    conn = get_connection()
    conn.execute(
        """CREATE TABLE IF NOT EXISTS channels(
            id TEXT PRIMARY KEY,
            title TEXT,
            description TEXT,
            color TEXT
            )"""
    )
    conn.execute(
    """CREATE TABLE IF NOT EXISTS items (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        channel_id TEXT,
        text TEXT,
        timestamp TEXT,
        read INTEGER DEFAULT 0
        )"""
    )
    conn.commit()
    conn.close()

def add_channel(channel_id, title, description, color):
    conn = get_connection()
    conn.execute(
        "INSERT OR IGNORE INTO channels (id, title, description, color) VALUES (?, ?, ?, ?)",
        (channel_id, title, description, color)
    )
    conn.commit()
    conn.close()

def update_channel(channel_id, title = None, color = None):
    conn = get_connection()
    if title is not None:
        conn.execute("UPDATE channels SET title = ? WHERE id = ?", (title, channel_id))
    if color is not None:
            conn.execute("UPDATE channels SET color = ? WHERE id = ?", (color, channel_id))
    conn.commit()
    conn.close()

def get_channel(channel_id):
    conn = get_connection()
    cur = conn.execute(
        "SELECT id, title, description, color FROM channels WHERE id = ?",
        (channel_id,)
    )
    row = cur.fetchone()
    conn.close()
    return row

def get_all_channels():
    conn = get_connection()
    cur = conn.execute("SELECT id, title, description, color FROM channels")
    rows = cur.fetchall()
    conn.close()
    return rows

def add_item(channel_id, text, timestamp):
    conn = get_connection()
    conn.execute(
        "INSERT INTO items (channel_id, text, timestamp) VALUES (?, ?, ?)",
        (channel_id, text, timestamp)
    )
    conn.commit()
    conn.close()

def get_unread_count(channel_id):
    conn = get_connection()
    cur = conn.execute(
        "SELECT COUNT(*) FROM items WHERE channel_id = ? AND read = 0",
        (channel_id,)
    )
    count = cur.fetchone()[0]
    conn.close()
    return count

def mark_read(item_id):
    conn = get_connection()
    conn.execute(
        "UPDATE items SET read = 1 WHERE id = ?", (item_id,)
    )
    conn.commit()
    conn.close()

if __name__ == "__main__":
    init_db()
    add_channel("deals", "Game Deals", "Free & discounted games", "#4CAF50")
    add_item("deals", "Fake test deal - Portal 2 is free!", "2026-09-25T12:00:00")
    print("Unread count:", get_unread_count("deals"))
    
    update_channel("deals", title = "Bargain Hunter")
    print("After rename: ", get_channel("deals"))
