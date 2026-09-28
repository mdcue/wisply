from PyQt6.QtWidgets import QWidget, QVBoxLayout, QLabel
from PyQt6.QtCore import Qt, QPoint
from PyQt6.QtGui import QGuiApplication

from core import db

SCREEN_MARGIN = 8

class PopupWindow(QWidget):
    def __init__(self):
        super().__init__()
        self.setWindowFlags(Qt.WindowType.Popup | Qt.WindowType.FramelessWindowHint)
        self.setFixedWidth(260)
        self.setStyleSheet("background-color : #1e1e1e; border-radius : 8px;")
        
        self.layout_ = QVBoxLayout(self)
        self.layout_.setContentsMargins(12, 12, 12, 12)
        self.layout_.setSpacing(8)
        
        title = QLabel("wisply")
        title.setStyleSheet("color : white; font-size : 14px; font-weight : bold;")
        self.layout_.addWidget(title)
    
    def refresh(self):
        while self.layout_.count() > 1:
            item = self.layout_.takeAt(1)
            widget = item.widget()
            if widget:
                widget.deleteLater()
        
        for channel_id, channel_title, description, color in db.get_all_channels():
            unread = db.get_unread_count(channel_id)
            row = QLabel(f"{channel_title}      {unread} unread")
            row.setStyleSheet(
                f"color : white; padding : 8px; border-left : 4px solid {color}; "
                f"background-color : rgba(255, 255, 255, 15);"
            )
            self.layout_.addWidget(row)
    
    def show_near(self, x, y):
        self.refresh()
        self.adjustSize()
        
        screen = QGuiApplication.screenAt(QPoint(x, y)) or QGuiApplication.primaryScreen()
        area = screen.availableGeometry()
        
        px = x
        py = y - self.height()
        
        min_x = area.x() + SCREEN_MARGIN
        max_x = area.x() + area.width() - self.width() - SCREEN_MARGIN
        min_y = area.y() + SCREEN_MARGIN
        max_y = area.y() + area.height() - self.height() - SCREEN_MARGIN
        
        px = max(min_x, min(px, max_x))
        py = max(min_y, min(py, max_y))
        
        self.move(px, py)
        self.show()
        self.activateWindow()