import sys
from pathlib import Path
from PyQt6.QtWidgets import QApplication, QSystemTrayIcon, QMenu
from PyQt6.QtGui import QIcon, QAction

from ui.popup import PopupWindow
from core import db, startup, scheduler

ASSETS_DIR = Path(__file__).parent / "assets"
ICON_PLAIN = ASSETS_DIR / "icon.png"
ICON_BADGE = ASSETS_DIR / "icon_badge.png"

class TrayApp:
    def __init__(self):
        self.app = QApplication(sys.argv)
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
            self.popup = PopupWindow()
        
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
                channel = db.get_channel(channel_id)
                bot_name = channel[1] if channel else channel_id
                self.tray.showMessage(
                    "wisply",
                    f"{bot_name} {message_suffix}",
                    QSystemTrayIcon.MessageIcon.Information,
                )
                
        self.old_counts = new_counts
        self.update_badge()

    def run(self):
        scheduler.start(on_complete = self.check_for_new_items)
        sys.exit(self.app.exec())

if __name__ == "__main__":
    db.init_db()
    db.add_channel("deals", "Game Deals", "Discounted games", "#4CAF50")
    db.add_channel("free_games", "Free Games", "Games that are free right now", "#2196F3")
    
    tray_app = TrayApp()
    tray_app.run()
