import sys
from pathlib import Path
from PyQt6.QtWidgets import QApplication, QMainWindow
from PyQt6.QtWebEngineWidgets import QWebEngineView
from PyQt6.QtWebChannel import QWebChannel
from PyQt6.QtCore import QUrl
from PyQt6.QtGui import QIcon

from core import db
from bots.registry import ALL_BOTS
from ui.bridge import HubBridge

ASSETS_DIR = Path(__file__).parent / "assets"
ICON_PLAIN = ASSETS_DIR / "icon.png"
DASHBOARD_HTML = Path(__file__).parent / "hub_web" / "dashboard.html"

class HubWindow(QMainWindow):
    def __init__(self, on_unread_changed = None):
        super().__init__()
        self.on_unread_changed = on_unread_changed
        
        self.setWindowTitle("wisply Hub")
        self.setWindowIcon(QIcon(str(ICON_PLAIN)))
        self.resize(420, 600)
        
        self.view = QWebEngineView()
        self.setCentralWidget(self.view)
        
        self.bridge = HubBridge()
        self.channel = QWebChannel()
        self.channel.registerObject("bridge", self.bridge)
        self.view.page().setWebChannel(self.channel)
        
        if self.on_unread_changed:
            self.bridge.dataChanged.connect(self.on_unread_changed)

        self.view.load(QUrl.fromLocalFile(str(DASHBOARD_HTML)))

if __name__ == "__main__":
    app = QApplication(sys.argv)
    db.init_db()
    for bot in ALL_BOTS:
        bot.register()
    
    hub = HubWindow()
    hub.show()
    
    sys.exit(app.exec())
