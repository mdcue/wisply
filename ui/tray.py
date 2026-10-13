import sys
import threading
from PyQt6.QtWidgets import QApplication, QSystemTrayIcon, QMenu
from PyQt6.QtGui import QIcon, QAction
from PyQt6.QtCore import QObject, pyqtSignal

from ui.bridge import HubBridge
from ui.launcher import LauncherStrip
from ui.hub import HubWindow
from ui.single_instance import SingleInstance
from core import db, startup, scheduler, appearance, account, logsetup
from core.paths import BASE_DIR
from bots.registry import ALL_BOTS

ASSETS_DIR = BASE_DIR / "ui" / "assets"
ICON_PLAIN = ASSETS_DIR / "icon.png"
ICON_BADGE = ASSETS_DIR / "icon_badge.png"

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
        
        self.update_badge()
        self.tray.show()
    
    def _snapshot_counts(self):
        counts = {}
        for bot_id, name, color, active, icon_path, expression, fetch_interval, last_fetched in db.get_all_bots():
            for channel_id, title, _desc, _color in db.get_bot_channels(bot_id):
                counts[channel_id] = db.get_unread_count(channel_id)
        return counts
    
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
    
    def toggle_startup(self):
        startup.toggle()
        self.startup_action.setChecked(startup.is_enabled())
    
    def update_badge(self):
        total_unread = sum(
            db.get_bot_unread(bot_id)
            for bot_id, name, color, active, icon_path, expression, fetch_interval, last_fetched in db.get_active_bots()
        )
        icon_path = ICON_BADGE if total_unread > 0 else ICON_PLAIN
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
                    f"{name} found something new",
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
