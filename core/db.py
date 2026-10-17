import sqlite3
from collections import namedtuple

from core.paths import DATA_DIR

DB_PATH = DATA_DIR / "data.db"

#Rows come back as named tuples. They still unpack and index like plain tuples, so existing
#code keeps working, but new code can say bot.color instead of bot[2].
Bot = namedtuple("Bot", "id name color active icon_path expression fetch_interval last_fetched")
Channel = namedtuple("Channel", "id title description color")
Item = namedtuple("Item", "id text read url price discount")
FeedEntry = namedtuple("FeedEntry", "item_id text read url channel_id bot_id bot_name icon_path")

BOT_COLUMNS = "id, name, color, active, icon_path, expression, fetch_interval_seconds, last_fetched_at"

#Columns added after the first release; older databases get them on startup.
MIGRATIONS = {
    "bots": {
        "active": "INTEGER DEFAULT 1",
        "icon_path": "TEXT",
        "expression": "TEXT DEFAULT 'smiling'",
        "fetch_interval_seconds": "INTEGER DEFAULT 3600",
        "last_fetched_at": "TEXT",
    },
    "channels": {
        "bot_id": "TEXT",
        "position": "INTEGER DEFAULT 0",
    },
    "items": {
        "url": "TEXT",
        "price": "REAL",
        "discount": "INTEGER",
        "dismissed": "INTEGER DEFAULT 0",
    },
}

def get_connection():
    return sqlite3.connect(DB_PATH)

def _read(sql, params = ()):
    conn = get_connection()
    try:
        return conn.execute(sql, params).fetchall()
    finally:
        conn.close()

def _write(sql, params = ()):
    conn = get_connection()
    try:
        conn.execute(sql, params)
        conn.commit()
    finally:
        conn.close()

def init_db():
    conn = get_connection()
    try:
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
            
        for table, columns in MIGRATIONS.items():
            existing = {row[1] for row in conn.execute(f"PRAGMA table_info({table})")}
            for name, definition in columns.items():
                if name not in existing:
                    conn.execute(f"ALTER TABLE {table} ADD COLUMN {name} {definition}")
            
        #these are precautions, without these every unread count, item list and duplicate check reads the whole items table
        conn.execute("CREATE INDEX IF NOT EXISTS idx_items_channel ON items(channel_id, id)")
        conn.execute("CREATE INDEX IF NOT EXISTS idx_items_unread ON items(channel_id, read)")
        conn.execute("CREATE INDEX IF NOT EXISTS idx_items_timestamp ON items(timestamp)")
        conn.execute("CREATE INDEX IF NOT EXISTS idx_channels_bot ON channels(bot_id)")
        conn.commit()
    finally:
        conn.close()

#bots (wisps)
def add_bot(bot_id, name, color, icon_path = None):
    _write(
        "INSERT OR IGNORE INTO bots (id, name, color, icon_path, active) VALUES (?, ?, ?, ?, 0)",
        (bot_id, name, color, icon_path)
    )

def get_bot(bot_id):
    rows = _read(f"SELECT {BOT_COLUMNS} FROM bots WHERE id = ?", (bot_id,))
    return Bot(*rows[0]) if rows else None

def get_all_bots():
    return [Bot(*row) for row in _read(f"SELECT {BOT_COLUMNS} FROM bots ORDER BY rowid")]

def get_active_bots():
    return [Bot(*row) for row in _read(f"SELECT {BOT_COLUMNS} FROM bots WHERE active = 1 ORDER BY rowid")]

def set_fetch_interval(bot_id, seconds):
    _write("UPDATE bots SET fetch_interval_seconds = ? WHERE id = ?", (seconds, bot_id))

def mark_fetched(bot_id):
    _write("UPDATE bots SET last_fetched_at = datetime('now') WHERE id = ?", (bot_id,))

def update_bot(bot_id, name = None, color = None, icon_path = None, expression = None):
    changes = {"name": name, "color": color, "icon_path": icon_path, "expression": expression}
    changes = {column: value for column, value in changes.items() if value is not None}
    if not changes:
        return
    assignments = ", ".join(f"{column} = ?" for column in changes) #column names come from the dictionary above, never from user input
    _write(f"UPDATE bots SET {assignments} WHERE id = ?", (*changes.values(), bot_id))

def set_bot_active(bot_id, active):
    _write("UPDATE bots SET active = ? WHERE id = ?", (1 if active else 0, bot_id))

def get_bot_channels(bot_id):
    rows = _read(
        "SELECT id, title, description, color FROM channels WHERE bot_id = ? ORDER BY position, rowid",
        (bot_id,)
    )
    return [Channel(*row) for row in rows]

def get_bot_unread(bot_id):
    #total unread across all of a bot's (wisp's) channels
    rows = _read(
        "SELECT COUNT(*) FROM items JOIN channels ON items.channel_id = channels.id "
        "WHERE channels.bot_id = ? AND items.read = 0",
        (bot_id,)
    )
    return rows[0][0]

#channels (for bots/wisps)
def add_channel(channel_id, title, description, color, bot_id = None, position = 0):
    _write(
    """INSERT INTO channels (id, title, description, color, bot_id, position)
    VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(id) DO UPDATE SET
        title = excluded.title,
        description = excluded.description,
        bot_id = COALESCE(excluded.bot_id, channels.bot_id),
        position = excluded.position
    """, (channel_id, title, description, color, bot_id, position)
    ) #COALESCE keeps an existing bot_id when a caller doesn't pass one-

#items for the database
def add_item(channel_id, text, timestamp, url = None, price = None, discount = None):
    _write(
        "INSERT INTO items (channel_id, text, timestamp, url, price, discount) VALUES (?, ?, ?, ?, ?, ?)",
        (channel_id, text, timestamp, url, price, discount)
    )

def get_unread_count(channel_id):
    return _read("SELECT COUNT(*) FROM items WHERE channel_id = ? AND read = 0", (channel_id,))[0][0]

def get_unread_counts():
    """Unread items per channel, for every channel that has any, in one query."""
    return dict(_read("SELECT channel_id, COUNT(*) FROM items WHERE read = 0 GROUP BY channel_id"))

def get_items(channel_id, limit = None):
    """A channel's items, newest first. `limit` caps how many come back (None means all)."""
    rows = _read(
        "SELECT id, text, read, url, price, discount FROM items WHERE channel_id = ? AND dismissed = 0 ORDER BY id DESC LIMIT ?",
        (channel_id, -1 if limit is None else limit)
    )
    return [Item(*row) for row in rows]

def get_recent_items(limit = 30):
    """The newest items across every active wisp, newest first each with the wisp it came from."""
    rows = _read(
        "SELECT items.id, items.text, items.read, items.url, items.channel_id, "
        "bots.id, bots.name, bots.icon_path "
        "FROM items JOIN channels ON items.channel_id = channels.id "
        "JOIN bots ON channels.bot_id = bots.id "
        "WHERE bots.active = 1 AND items.dismissed = 0 ORDER BY items.id DESC LIMIT ?",
        (limit,)
    )
    return [FeedEntry(*row) for row in rows]

def has_item(key):
    """True if an item with this duplicate-check key is already sorted, in any channel."""
    return bool(_read("SELECT 1 FROM items WHERE timestamp = ? LIMIT 1", (key,)))

def mark_read(item_id):
    _write("UPDATE items SET read = 1 WHERE id = ?", (item_id,))

def mark_all_read(channel_id):
    _write("UPDATE items SET read = 1 WHERE channel_id = ? AND read = 0", (channel_id,))

def dismiss_item(item_id):
    """Removes an item from every list for good. It is also marked read. The row stays, with
    its duplicate-check key, so the same item is never added again."""
    _write("UPDATE items SET read = 1, dismissed = 1 WHERE id = ?", (item_id,))

def dismiss_read(channel_id):
    """Removes every read item in a channel from view (see dismiss_item)."""
    _write("UPDATE items SET dismissed = 1 WHERE channel_id = ? AND read = 1", (channel_id,))
