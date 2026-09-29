import sys
from pathlib import Path
from PyQt6.QtWidgets import QApplication, QSystemTrayIcon, QMenu
from PyQt6.QtGui import QIcon, QAction
from PyQt6.QtCore import QObject, pyqtSignal

from ui.popup import PopupWindow
from core import db, startup, scheduler
from bots import deals

ASSETS_DIR = Path(__file__).parent / "assets"
ICON_PLAIN = ASSETS_DIR / "icon.png"
ICON_BADGE = ASSETS_DIR / "icon_badge.png"

class SchedulerSignals(QObject):
    cycle_complete = pyqtSignal()
class TrayApp:
    def __init__(self):
        self.app = QApplication(sys.argv)
        self.signals = SchedulerSignals()
        self.signals.cycle_complete.connect(self.check_for_new_items)
        self.app.setQuitOnLastWindowClosed(False) #this is for popup closing
        
        self.tray = QSystemTrayIcon(QIcon(str(ICON_PLAIN)))
        self.tray.setToolTip("wisply")
        
        self.popup = None
        
        self.menu = QMenu()
        
        self.startup_action = QAction("Start with Windows", checkable = True)
        self.startup_action.setChecked(startup.is_enabled())
        self.startup_action.triggered.connect(self.toggle_startup)
        self.menu.addAction(self.startup_action)
        
        self.quit_action = QAction("Quit")
        self.quit_action.triggered.connect(self.app.quit)
        self.menu.addAction(self.quit_action)
        
        self.tray.setContextMenu(self.menu)
        self.tray.activated.connect(self.on_tray_activated)
        
        self.old_counts = {
            "deals" : db.get_unread_count("deals"),
            "free_games" : db.get_unread_count("free_games"),
        }
        
        self.update_badge()
        self.tray.show()

    def on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.toggle_popup()

    def toggle_popup(self):
        if self.popup is None:
            self.popup = PopupWindow(on_unread_changed = self.on_unread_changed)
        
        if self.popup.isVisible():
            self.popup.hide()
            return
        
        geo = self.tray.geometry()
        x = geo.x()
        y = geo.y()
        self.popup.show_near(x, y)

    def toggle_startup(self):
        startup.toggle()
        self.startup_action.setChecked(startup.is_enabled())

    def update_badge(self):
        total_unread = db.get_unread_count("deals") + db.get_unread_count("free_games")
        icon_path = ICON_BADGE if total_unread > 0 else ICON_PLAIN
        self.tray.setIcon(QIcon(str(icon_path)))

    def on_unread_changed(self):
        self.old_counts = {
            "deals" : db.get_unread_count("deals"),
            "free_games" : db.get_unread_count("free_games"),
        }
        self.update_badge()

    def check_for_new_items(self):
        new_counts = {
            "deals" : db.get_unread_count("deals"),
            "free_games" : db.get_unread_count("free_games"),
        }
        for channel_id, message_suffix in(
            ("deals", "found a new deal"),
            ("free_games", "a new free game is available"),
        ):
            if new_counts[channel_id] > self.old_counts[channel_id]:
                bot = db.get_bot_for_channel(channel_id)
                bot_name = bot[1] if bot else channel_id
                self.tray.showMessage(
                    "wisply",
                    f"{bot_name} {message_suffix}",
                    QSystemTrayIcon.MessageIcon.Information,
                )
                
        self.old_counts = new_counts
        self.update_badge()
    
    def _on_cycle_complete(self):
        self.signals.cycle_complete.emit()

    def run(self):
        scheduler.start(on_complete = self._on_cycle_complete)
        sys.exit(self.app.exec())

if __name__ == "__main__":
    db.init_db()
    deals.register()
    
    tray_app = TrayApp()
    tray_app.run()
