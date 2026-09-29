import sqlite3
from pathlib import Path

DB_PATH = Path(__file__).parent.parent / "data.db"
#__file__ is used so it can hold the path to the current file.

def get_connection():
    return sqlite3.connect(DB_PATH)

def init_db():
    conn = get_connection()
    #For reference, a bot is the identity (name + color); channels are its feeds
    conn.execute(
    """CREATE TABLE IF NOT EXISTS bots (
        id TEXT PRIMARY KEY,
        name TEXT,
        color TEXT
    )
    """)
    conn.execute(
    """CREATE TABLE IF NOT EXISTS channels (
        id TEXT PRIMARY KEY,
        title TEXT,
        description TEXT,
        color TEXT,
        bot_id TEXT
    )
    """)
    conn.execute(
    """CREATE TABLE IF NOT EXISTS items (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        channel_id TEXT,
        text TEXT,
        timestamp TEXT,
        read INTEGER DEFAULT 0
    )
    """)
    
    columns = [row[1] for row in conn.execute("PRAGMA table_info(channels)")]
    if "bot_id" not in columns:
        conn.execute("ALTER TABLE channels ADD COLUMN bot_id TEXT")
    if "position" not in columns:
        conn.execute("ALTER TABLE channels ADD COLUMN position INTEGER DEFAULT 0")
    item_columns = [row[1] for row in conn.execute("PRAGMA table_info(items)")]
    if "url" not in item_columns:
        conn.execute("ALTER TABLE items ADD COLUMN url TEXT")
    conn.commit()
    conn.close()

#bots
def add_bot(bot_id, name, color):
    conn = get_connection()
    conn.execute(
        "INSERT OR IGNORE INTO bots (id, name, color) VALUES (?, ?, ?)",
        (bot_id, name, color)
    )
    conn.commit()
    conn.close()

def get_bot(bot_id):
    conn = get_connection()
    row = conn.execute(
        "SELECT id, name, color FROM bots WHERE id = ?", (bot_id,)
    ).fetchone()
    conn.close()
    return row

def get_all_bots():
    conn = get_connection()
    rows = conn.execute(
        "SELECT id, name, color FROM bots ORDER BY rowid"
    ).fetchall()
    conn.close()
    return rows

def update_bot (bot_id, name = None, color = None):
    conn = get_connection()
    if name is not None:
        conn.execute("UPDATE bots SET name = ? WHERE id = ?", (name, bot_id))
    if color is not None:
        conn.execute("UPDATE bots SET color = ? WHERE id = ?", (color, bot_id))
    conn.commit()
    conn.close()

def get_bot_channels(bot_id):
    conn = get_connection()
    rows = conn.execute(
        "SELECT id, title, description, color FROM channels WHERE bot_id = ? ORDER BY position, rowid",
        (bot_id,)
    ).fetchall()
    conn.close()
    return rows

def get_bot_for_channel(channel_id):
    #this is for toasts, which bot does this channel belong to- to check that
    conn = get_connection()
    row = conn.execute(
        "SELECT bots.id, bots.name, bots.color FROM bots "
        "JOIN channels ON channels.bot_id = bots.id WHERE channels.id = ?",
        (channel_id,)
    ).fetchone()
    conn.close()
    return row

def get_bot_unread(bot_id):
    #total unread across all of a bot's channels
    conn = get_connection()
    count = conn.execute(
        "SELECT COUNT(*) FROM items JOIN channels ON items.channel_id = channels.id "
        "WHERE channels.bot_id = ? AND items.read = 0",
        (bot_id,)
    ).fetchone()[0]
    conn.close()
    return count

#channels (for bots)
def add_channel(channel_id, title, description, color, bot_id = None, position = 0):
    conn = get_connection()
    conn.execute(
    """INSERT INTO channels (id, title, description, color, bot_id, position)
    VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(id) DO UPDATE SET
        title = excluded.title,
        description = excluded.description,
        bot_id = COALESCE(excluded.bot_id, channels.bot_id),
        position = excluded.position
    """,(channel_id, title, description, color, bot_id, position)
    )#COALESCE keeps an existing bot_id when a caller doesn't pass one.
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
    row = conn.execute(
        "SELECT id, title, description, color FROM channels WHERE id = ?",
        (channel_id,)
    ).fetchone()
    conn.close()
    return row

def get_all_channels():
    conn = get_connection()
    rows = conn.execute("SELECT id, title, description, color FROM channels").fetchall()
    conn.close()
    return rows

#items for the database
def add_item(channel_id, text, timestamp, url = None):
    conn = get_connection()
    conn.execute(
        "INSERT INTO items (channel_id, text, timestamp, url) VALUES (?, ?, ?, ?)",
        (channel_id, text, timestamp, url)
    )
    conn.commit()
    conn.close()

def get_unread_count(channel_id):
    conn = get_connection()
    count = conn.execute(
        "SELECT COUNT(*) FROM items WHERE channel_id = ? AND read = 0",
        (channel_id,)
    ).fetchone()[0]
    conn.close()
    return count

def get_items(channel_id):
    conn = get_connection()
    rows = conn.execute(
        "SELECT id, text, read, url FROM items WHERE channel_id = ? ORDER BY id DESC",
        (channel_id,)
    ).fetchall()
    conn.close()
    return rows

def mark_read(item_id):
    conn = get_connection()
    conn.execute("UPDATE items SET read = 1 WHERE id = ?", (item_id,))
    conn.commit()
    conn.close()

def mark_all_read(channel_id):
    conn = get_connection()
    conn.execute(
        "UPDATE items SET read = 1 WHERE channel_id = ? AND read = 0",
        (channel_id,)
    )
    conn.commit()
    conn.close()
