import base64
import json
import threading
from pathlib import Path
from PyQt6.QtCore import QObject, pyqtSlot, pyqtSignal, QUrl
from PyQt6.QtGui import QDesktopServices

from core import db, account, appearance, icons
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

class HubBridge(QObject):
    dataChanged = pyqtSignal()
    newItemsFound = pyqtSignal(str, int)
    
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
            for item_id, text, read, url, price, discount in db.get_items(channel_id)[:10]:
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
    
    @pyqtSlot(int, str)
    def open_item(self, item_id, url):
        db.mark_read(item_id)
        QDesktopServices.openUrl(QUrl(url))
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
                "icon": _icon_to_data_uri(icon_path),
                "fetch_interval": fetch_interval,
                "last-fetched": last_fetched or "",
            })
        return json.dumps(wisps)

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
                threading.Thread(target = self._fetch_then_notify, args = (module,), daemon = True).start()
        
        return new_state
        
    def _fetch_then_notify(self, module):
        module.fetch()
        self.dataChanged.emit()
