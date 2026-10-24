import json
import sys
import threading

from PyQt6.QtWidgets import QApplication, QSystemTrayIcon, QMenu, QMessageBox
from PyQt6.QtGui import QIcon, QAction
from PyQt6.QtCore import QObject, Qt, QTimer, pyqtSignal

from ui.bridge import HubBridge
from ui.launcher import LauncherStrip
from ui.hub import HubWindow
from ui.single_instance import SingleInstance
from ui.quick_add import ask_for_note
from core import db, startup, scheduler, appearance, account, logsetup
from core.paths import BASE_DIR
from bots import companion, companion_voice
from bots.registry import ALL_BOTS

ASSETS_DIR = BASE_DIR / "ui" / "assets"
ICON_PLAIN = ASSETS_DIR / "icon.png"
ICON_BADGE = ASSETS_DIR / "icon_badge.png"

def set_app_identity(app):
    """Gives Windows our own identity, so the taskbar shows the Wisply icon instead of Python's."""
    if sys.platform == "win32":
        import ctypes
        ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID("Wisply.App")
    app.setWindowIcon(QIcon(str(ICON_PLAIN)))

class SchedulerSignals(QObject):
    cycle_complete = pyqtSignal()

class TrayApp:
    def __init__(self):
        self.app = QApplication.instance() or QApplication(sys.argv)
        self.app.setQuitOnLastWindowClosed(False)
        
        self.signals = SchedulerSignals()
        self.signals.cycle_complete.connect(self.check_for_new_items)
        
        self.tray = QSystemTrayIcon(QIcon(str(ICON_PLAIN)))
        self.tray.setToolTip("wisply")
        
        self.launcher = LauncherStrip(on_unread_changed = self.on_unread_changed)
        self.hub_bridge = HubBridge()
        self.hub = None
        
        self.menu = QMenu()
        
        self.hub_action = QAction("Open Hub")
        self.hub_action.triggered.connect(self.open_hub)
        self.menu.addAction(self.hub_action)
        self.menu.addSeparator()
        
        self.idea_action = QAction("Add idea...")
        self.idea_action.triggered.connect(self.add_idea)
        self.menu.addAction(self.idea_action)
        
        self.reminder_action = QAction("Add reminder...")
        self.reminder_action.triggered.connect(self.add_reminder)
        self.menu.addAction(self.reminder_action)
        self.menu.addSeparator()
        
        self.startup_action = QAction("Start with Windows", checkable = True)
        self.startup_action.setChecked(startup.is_enabled())
        self.startup_action.triggered.connect(self.toggle_startup)
        self.menu.addAction(self.startup_action)
        
        self.quit_action = QAction("Quit")
        self.quit_action.triggered.connect(self.app.quit)
        self.menu.addAction(self.quit_action)
        
        self.tray.setContextMenu(self.menu)
        self.tray.activated.connect(self.on_tray_activated)
        
        self.old_counts = self._snapshot_counts()
        
        self.badge_shown = None
        self.update_badge()
        self.tray.show()
    
    def _snapshot_counts(self):
        return db.get_unread_counts()
    
    def on_tray_activated(self, reason):
        if reason == QSystemTrayIcon.ActivationReason.Trigger:
            self.toggle_launcher()
    
    def toggle_launcher(self):
        if self.launcher.isVisible():
            self.launcher.hide()
            return
        geo = self.tray.geometry()
        self.launcher.show_near(geo.x(), geo.y())
    
    def open_hub(self):
        if self.hub is None or not self.hub.isVisible():
            self.hub = HubWindow(on_unread_changed = self.on_unread_changed, bridge = self.hub_bridge)
        self.hub.show()
        self.hub.raise_()
        self.hub.activateWindow()
    
    def add_idea(self):
        values = ask_for_note(companion_voice.IDEA_WINDOW, with_time = False)
        if values:
            self._save_note(lambda: companion.create_idea(values[0]), companion_voice.SAVED_IDEA)
    
    def add_reminder(self):
        values = ask_for_note(companion_voice.REMINDER_WINDOW, with_time = True)
        if values:
            text, date_text, time_text, repeat = values
            self._save_note(lambda: companion.create_reminder(text, date_text, time_text, repeat), companion_voice.SAVED_REMINDER)
    
    def _save_note(self, action, saved_message):
        """Saves a note from the tray menu, and says so if it could not be saved."""
        bot = db.get_bot(companion.BOT_ID)
        if not bot or not bot.active:
            self._tell(companion_voice.COMPANION_OFF)
            return
        result = json.loads(self.hub_bridge._companion_change(action))
        if result["ok"]:
            self.tray.showMessage(bot.name, saved_message, QSystemTrayIcon.MessageIcon.Information)
            QTimer.singleShot(2000, self.check_for_new_items)
        else:
            self._tell(result["error"])
    
    def _tell(self, message):
        box = QMessageBox()
        box.setWindowTitle("wisply")
        box.setText(message)
        box.setWindowFlag(Qt.WindowType.WindowStaysOnTopHint, True)
        box.exec()
    
    def toggle_startup(self):
        startup.toggle()
        self.startup_action.setChecked(startup.is_enabled())
    
    def update_badge(self):
        total_unread = sum(
            db.get_bot_unread(bot_id)
            for bot_id, name, color, active, icon_path, expression, fetch_interval, last_fetched in db.get_active_bots()
        )
        has_unread = total_unread > 0
        if has_unread == self.badge_shown:
            return
        self.badge_shown = has_unread
        icon_path = ICON_BADGE if has_unread else ICON_PLAIN
        self.tray.setIcon(QIcon(str(icon_path)))
    
    def on_unread_changed(self):
        self.old_counts = self._snapshot_counts()
        self.update_badge()
    
    def _on_cycle_complete(self):
        self.signals.cycle_complete.emit()
    
    def check_for_new_items(self):
        new_counts = self._snapshot_counts()
        
        for bot_id, name, color, active, icon_path, expression, fetch_interval, last_fetched in db.get_active_bots():
            bot_new_count = 0
            for channel_id, title, _desc, _color in db.get_bot_channels(bot_id):
                old = self.old_counts.get(channel_id, 0)
                new = new_counts.get(channel_id, 0)
                if new > old:
                    bot_new_count += (new - old)
            
            if bot_new_count > 0:
                self.tray.showMessage(
                    "wisply",
                    companion_voice.new_items(name),
                    QSystemTrayIcon.MessageIcon.Information,
                )
                self.hub_bridge.newItemsFound.emit(name, bot_new_count)
        
        self.old_counts = new_counts
        self.update_badge()
    
    def run(self):
        scheduler.start(on_complete = self._on_cycle_complete)
        sys.exit(self.app.exec())

def main():
    logsetup.setup_logging()
    background = "--background" in sys.argv
    
    app = QApplication.instance() or QApplication(sys.argv) #needs to exist before SingleInstance; the name keeps it alive
    set_app_identity(app)
    single = SingleInstance()
    if single.notify_existing(show = not background):
        print("Wisply is already running.")
        return
    if not single.start_listening():
        print("Could not register as the running instance; continuing anyway.")
    
    db.init_db()
    for bot in ALL_BOTS:
        bot.register()
    appearance.ensure_icons()
    threading.Thread(target = scheduler.fetch_startup_only_bots, daemon = True).start()
    
    tray_app = TrayApp()
    tray_app.single_instance = single #keeps the listener alive for as long as the app runs
    single.show_requested.connect(tray_app.open_hub)
    if not background or not account.account_exists():
        tray_app.open_hub()
    tray_app.run()

if __name__ == "__main__":
    main()
