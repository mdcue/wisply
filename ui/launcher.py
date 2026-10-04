import sys
from pathlib import Path
from PyQt6.QtWidgets import QWidget, QHBoxLayout, QPushButton, QApplication
from PyQt6.QtCore import Qt, QPoint, QSize
from PyQt6.QtGui import QIcon, QGuiApplication

from core import db
from ui.popup import PopupWindow, clamp, SCREEN_MARGIN

ICON_BUTTON_SIZE = 44
ICON_SIZE = 32
LAUNCHER_STYLE = "LauncherStrip { background-color: #1e1e1e; border-radius: 8px; }"
BOT_BUTTON_STYLE = "border: none; border-radius: 8px; background-color: rgba(255, 255, 255, 15);"

class LauncherStrip(QWidget):
    """A small row of active wisps' icon(s), opened from the tray's left-click.
    Clicking a bot's icon opens that one bot's own popup."""
    
    def __init__(self, on_unread_changed = None):
        super().__init__()
        self.on_unread_changed = on_unread_changed
        self.anchor = (0, 0)
        self.bot_popups = {}
        self.empty_popup = None
        
        self.setWindowFlags(Qt.WindowType.Popup | Qt.WindowType.FramelessWindowHint)
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, True)
        self.setObjectName("LauncherStrip")
        self.setStyleSheet(LAUNCHER_STYLE)
        
        self.layout_ = QHBoxLayout(self)
        self.layout_.setContentsMargins(8, 8, 8, 8)
        self.layout_.setSpacing(6)
    
    def refresh(self):
        while self.layout_.count():
            widget = self.layout_.takeAt(0).widget()
            if widget:
                widget.hide()
                widget.deleteLater()
        
        for bot_id, name, color, active, icon_path, expression, fetch_interval, last_fetched in db.get_active_bots():
            btn = QPushButton()
            btn.setFixedSize(ICON_BUTTON_SIZE, ICON_BUTTON_SIZE)
            btn.setCursor(Qt.CursorShape.PointingHandCursor)
            btn.setToolTip(name)
            btn.setStyleSheet(BOT_BUTTON_STYLE)
            if icon_path and Path(icon_path).exists():
                btn.setIcon(QIcon(icon_path))
                btn.setIconSize(QSize(ICON_SIZE, ICON_SIZE))
            btn.clicked.connect(lambda _c = False, bid = bot_id: self.open_bot(bid))
            self.layout_.addWidget(btn)
    
    def open_bot(self, bot_id):
        self.hide()
        popup = self.bot_popups.get(bot_id)
        if popup is None:
            popup = PopupWindow(on_unread_changed = self.on_unread_changed, bot_id = bot_id, popup_style = False)
            self.bot_popups[bot_id] = popup
        x, y = self.anchor
        popup.show_near(x, y)
    
    def show_near(self, x, y):
        self.anchor = (x, y)
        
        if not db.get_active_bots():
            if self.empty_popup is None:
                self.empty_popup = PopupWindow(on_unread_changed = self.on_unread_changed)
            self.empty_popup.show_near(x, y)
            return
        
        self.refresh()
        self.adjustSize()
        self._place(x, y)
        self.show()
        self.activateWindow()
    
    def _place(self, x, y):
        screen = QGuiApplication.screenAt(QPoint(x, y)) or QGuiApplication.primaryScreen()
        area = screen.availableGeometry()
        
        min_x = area.x() + SCREEN_MARGIN
        max_x = area.x() + area.width() - self.width() - SCREEN_MARGIN
        min_y = area.y() + SCREEN_MARGIN
        max_y = area.y() + area.height() - self.height() - SCREEN_MARGIN
        
        self.move(clamp(x, min_x, max_x), clamp(y - self.height(), min_y, max_y))

if __name__ == "__main__":
    #testing
    from core import db as _db
    from bots.registry import ALL_BOTS
    
    app = QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)
    _db.init_db()
    for bot in ALL_BOTS:
        bot.register()
    
    strip = LauncherStrip()
    strip.show_near(600, 600)
    
    sys.exit(app.exec())
