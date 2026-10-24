import base64
import json
import traceback
import threading
from pathlib import Path
from PyQt6.QtCore import QObject, pyqtSlot, pyqtSignal, QUrl, QTimer
from PyQt6.QtWidgets import QApplication
from PyQt6.QtGui import QDesktopServices
from datetime import datetime

from bots import companion, companion_voice
from bots.common import DEFAULT_INTERVALS
from core import db, account, appearance, icons
from core.paths import DATA_DIR
from bots.registry import BOT_MODULES

def _icon_to_data_uri(icon_path):
    """Reads an icon file and returns it as a base64 data URI, so the web page
    can display it via <img src"..."> with no file:// permission issues."""
    if not icon_path:
        return ""
    path = Path(icon_path)
    if not path.exists():
        return ""
    
    encoded = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:image/png;base64,{encoded}"

def _utc_to_local_text(utc_text):
    """SQLite's datetime ('now') is UTC. Show it in the user's own time zone."""
    if not utc_text:
        return ""
    try:
        parsed = datetime.strptime(utc_text, "%Y-%m-%d %H:%M:%S")
        return parsed.astimezone().strftime("%Y-%m-%d %H:%M:%S")
    except ValueError:
        return utc_text

class HubBridge(QObject):
    dataChanged = pyqtSignal()
    newItemsFound = pyqtSignal(str, int)
    fetchFinished = pyqtSignal(str, bool, int)
    
    @pyqtSlot(result = bool)
    def account_exists(self):
        return account.account_exists()
    
    @pyqtSlot(str, result = bool)
    def create_account(self, name):
        name = name.strip()
        if not name:
            return False
        account.create_account(name)
        return True
    
    @pyqtSlot(result = str)
    def get_expressions(self):
        return json.dumps(icons.get_available_expressions())
    
    @pyqtSlot(str, result = str)
    def get_channels(self, bot_id):
        channels = []
        for channel_id, title, _desc, color in db.get_bot_channels(bot_id):
            items = []
            for item_id, text, read, url, price, discount in db.get_items(channel_id, limit = 10):
                items.append({
                    "id": item_id, "text": text, "read": bool(read), "url": url or "",
                    "price": price, "discount": discount,
                })
            channels.append({
                "id": channel_id,
                "title": title,
                "color": color,
                "unread": db.get_unread_count(channel_id),
                "items": items,
            })
        return json.dumps(channels)
    
    @pyqtSlot(str)
    def mark_all_read(self, channel_id):
        db.mark_all_read(channel_id)
        self.dataChanged.emit()
    
    @pyqtSlot(result = str)
    def companion_notes(self):
        return json.dumps({
            "reminders": companion.list_reminders(),
            "ideas": companion.list_ideas(),
            "empty_reminders": companion_voice.EMPTY_REMINDERS,
            "empty_ideas": companion_voice.EMPTY_IDEAS,
        })
    
    @pyqtSlot(result = str)
    def companion_mood(self):
        return json.dumps(companion.describe_mood())
    
    @pyqtSlot(str, str, str, str, result = str)
    def companion_add_reminder(self, text, date_text, time_text, repeat):
        return self._companion_change(
            lambda: companion.create_reminder(text, date_text, time_text, repeat), companion_voice.SAVED_REMINDER
        )
    
    @pyqtSlot(str, result = str)
    def companion_add_idea(self, text):
        return self._companion_change(
            lambda: companion.create_idea(text), companion_voice.SAVED_IDEA
        )
    
    @pyqtSlot(int, str, str, str, str, result = str)
    def companion_update(self, note_id, text, date_text, time_text, repeat):
        return self._companion_change(lambda: companion.update_note(note_id, text, date_text, time_text, repeat))
    
    @pyqtSlot(int, result = str)
    def companion_complete(self, note_id):
        return self._companion_change(lambda: companion.complete_reminder(note_id))
    
    @pyqtSlot(int, str, result = str)
    def companion_snooze(self, note_id, option):
        return self._companion_change(lambda: companion.snooze_reminder(note_id, option))
    
    @pyqtSlot(int, result = str)
    def companion_delete(self, note_id):
        return self._companion_change(lambda: companion.delete_note(note_id))
    
    def _companion_change(self, action, message = None):
        """Runs a change to Companion's notes and answers {"ok": true} or {"ok": false, "error": "..."}
        with a message the page can show (on success, `message` is Companion's cheerful confirmation, if any).
        Then Companion checks right away, so its items and face react."""
        try:
            action()
        except ValueError as error:
            return json.dumps({"ok": False, "error": str(error)})
        threading.Thread(target = self._run_fetch, args = ("companion", companion, "Companion update"), daemon = True).start()
        self.dataChanged.emit()
        return json.dumps({"ok": True, "message": message})
    
    @pyqtSlot(int)
    def dismiss_item(self, item_id):
        db.dismiss_item(item_id)
        self.dataChanged.emit()
    
    @pyqtSlot(str)
    def clear_read(self, channel_id):
        db.dismiss_read(channel_id)
        self.dataChanged.emit()
    
    @pyqtSlot(int, str)
    def open_item(self, item_id, url):
        db.mark_read(item_id)
        QDesktopServices.openUrl(QUrl(url))
        self.dataChanged.emit()
    
    @pyqtSlot(int)
    def mark_item_read(self, item_id):
        db.mark_read(item_id)
        self.dataChanged.emit()
    
    @pyqtSlot(str, str, result = bool)
    def rename_wisp(self, bot_id, new_name):
        new_name = new_name.strip()
        if not new_name:
            return False
        db.update_bot(bot_id, name = new_name)
        self.dataChanged.emit()
        return True
    
    @pyqtSlot(str, str, result = bool)
    def recolor_wisp(self, bot_id, hex_color):
        appearance.set_bot_appearance(bot_id, color = hex_color)
        self.dataChanged.emit()
        return True
    
    @pyqtSlot(str, str, result = bool)
    def set_expression(self, bot_id, expression):
        appearance.set_bot_appearance(bot_id, expression = expression)
        self.dataChanged.emit()
        return True
    
    @pyqtSlot(str)
    def fetch_now(self, bot_id):
        module = BOT_MODULES.get(bot_id)
        if module:
            threading.Thread(target = self._run_fetch, args = (bot_id, module, "Manual check"), daemon = True).start()
    
    def _run_fetch(self, bot_id, module, reason):
        """Fetches one wisp on a background thread, records when it was checked, and tells
        the page how it went."""
        print(f"{reason} for {bot_id}...")
        before = db.get_bot_unread(bot_id)
        ok = False
        try:
            module.fetch()
            db.mark_fetched(bot_id)
            ok = True
        except Exception:
            traceback.print_exc()
        finally:
            after = db.get_bot_unread(bot_id)
            print(f"{reason} for {bot_id} {'finished' if ok else 'failed'}. Unread: {after}")
            self.dataChanged.emit()
            self.fetchFinished.emit(bot_id, ok, max(0, after - before))
    
    @pyqtSlot()
    def open_data_folder(self):
        QDesktopServices.openUrl(QUrl.fromLocalFile(str(DATA_DIR)))
    
    @pyqtSlot()
    def quit_app(self):
        QTimer.singleShot(0, lambda: QApplication.quit())
    
    @pyqtSlot(result = str)
    def get_wisps(self):
        wisps = []
        for bot_id, name, color, active, icon_path, expression, fetch_interval, last_fetched in db.get_all_bots():
            module = BOT_MODULES.get(bot_id)
            description = getattr(module, "DESCRIPTION", "")
            wisps.append({
                "id": bot_id,
                "name": name,
                "color": color,
                "active": bool(active),
                "icon": _icon_to_data_uri(icon_path),
                "expression": expression,
                "description": description,
            })
        return json.dumps(wisps)
    
    @pyqtSlot(result = str)
    def get_settings_wisps(self):
        wisps = []
        for bot_id, name, color, active, icon_path, expression, fetch_interval, last_fetched in db.get_active_bots():
            wisps.append({
                "id": bot_id,
                "name": name,
                "color": color,
                "icon": _icon_to_data_uri(icon_path),
                "fetch_interval": fetch_interval,
                "intervals": list(getattr(BOT_MODULES.get(bot_id), "FETCH_INTERVALS", DEFAULT_INTERVALS)),
                "last_fetched": _utc_to_local_text(last_fetched),
            })
        return json.dumps(wisps)

    @pyqtSlot(result = str)
    def get_activity_feed(self):
        icons_by_bot = {}
        feed = []
        for entry in db.get_recent_items(30):
            if entry.bot_id not in icons_by_bot:
                icons_by_bot[entry.bot_id] = _icon_to_data_uri(entry.icon_path)
            feed.append({
                "item_id": entry.item_id,
                "text": entry.text,
                "read": bool(entry.read),
                "url": entry.url or "",
                "wisp_name": entry.bot_name,
                "wisp_icon": icons_by_bot[entry.bot_id],
                "channel_id": entry.channel_id,
            })
        return json.dumps(feed)
    
    @pyqtSlot(str, int)
    def set_fetch_interval(self, bot_id, seconds):
        db.set_fetch_interval(bot_id, seconds)

    @pyqtSlot(str, result = bool)
    def toggle_active(self, bot_id):
        bot = db.get_bot(bot_id)
        if not bot:
            return False
        
        new_state = not bool(bot[3])
        db.set_bot_active(bot_id, new_state)
        self.dataChanged.emit()
        
        if new_state:
            module = BOT_MODULES.get(bot_id)
            if module:
                threading.Thread(target = self._run_fetch, args = (bot_id, module, "Activation check"), daemon = True).start()
        
        return new_state
        
    def _fetch_then_notify(self, module):
        try:
            module.fetch()
        except Exception:
            traceback.print_exc()
        finally:
            self.dataChanged.emit()
