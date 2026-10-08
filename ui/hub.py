import sys
from pathlib import Path
from PyQt6.QtWidgets import QApplication, QMainWindow
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWebChannel import QWebChannel
from PyQt6.QtWebEngineCore import QWebEnginePage, QWebEngineProfile
from PyQt6.QtCore import QUrl
from PyQt6.QtGui import QIcon

from core import db
from core.paths import BASE_DIR
from bots.registry import ALL_BOTS
from ui.bridge import HubBridge

ASSETS_DIR = BASE_DIR / "ui" / "assets"
ICON_PLAIN = ASSETS_DIR / "icon.png"
DASHBOARD_HTML = BASE_DIR / "ui" / "hub_web" / "dashboard.html"

class DebugPage(QWebEnginePage):
    def javaScriptConsoleMessage(self, level, message, line, source):
        print(f"JS CONSOLE [{line}]: {message}")

class HubWindow(QMainWindow):
    def __init__(self, on_unread_changed = None, bridge = None):
        super().__init__()
        self.on_unread_changed = on_unread_changed
        
        self.setWindowTitle("wisply Hub")
        self.setWindowIcon(QIcon(str(ICON_PLAIN)))
        self.resize(420, 600)
        
        self.view = QWebEngineView()
        
        self.profile = QWebEngineProfile(self.view)
        self.profile.setHttpCacheType(QWebEngineProfile.HttpCacheType.MemoryHttpCache)
        self.page = DebugPage(self.profile, self.view)
        self.view.setPage(self.page)
        self.setCentralWidget(self.view)
        
        self.bridge = bridge if bridge is not None else HubBridge()
        self.channel = QWebChannel()
        self.channel.registerObject("bridge", self.bridge)
        self.view.page().setWebChannel(self.channel)
        
        if self.on_unread_changed:
            self.bridge.dataChanged.connect(self.on_unread_changed)
        self.view.load(QUrl.fromLocalFile(str(DASHBOARD_HTML)))
    
    def closeEvent(self, event):
        self.view.setPage(None)
        try:
            self.page.deleteLater()
        except RuntimeError:
            pass
        try:
            self.profile.deleteLater()
        except RuntimeError:
            pass
        super().closeEvent(event)

if __name__ == "__main__":
    app = QApplication(sys.argv)
    db.init_db()
    for bot in ALL_BOTS:
        bot.register()
    
    hub = HubWindow()
    hub.show()
    
    sys.exit(app.exec())
