import base64
import json
import threading
from pathlib import Path
from PyQt6.QtCore import QObject, pyqtSlot, pyqtSignal

from core import db, account
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
    def get_wisps(self):
        wisps = []
        for bot_id, name, color, active, icon_path, expression in db.get_all_bots():
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
