import base64
import json
from pathlib import Path
from PyQt6.QtCore import QObject, pyqtSlot, pyqtSignal

from core import db
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
    """Exposes Python functions to the Hub's web page, and signals it when data changes."""
    
    dataChanged = pyqtSignal()
    
    @pyqtSlot(result = str)
    def get_wisps(self):
        wisps = []
        for bot_id, name, color, active, icon_path, expression in db.get_all_bots():
            wisps.append({
                "id": bot_id,
                "name": name,
                "color": color,
                "active": bool(active),
                "icon": _icon_to_data_uri(icon_path),
                "expression": expression,
            })
        return json.dumps(wisps)
    
    @pyqtSlot(str, result = bool)
    def toggle_active(self, bot_id):
        bot = db.get_bot(bot_id)
        if not bot:
            return False
        
        new_state = not bool(bot[3])
        db.set_bot_active(bot_id, new_state)
        
        if new_state:
            module = BOT_MODULES.get(bot_id)
            if module:
                module.fetch()
        
        self.dataChanged.emit()
        return new_state
