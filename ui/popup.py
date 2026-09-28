from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QLabel, QPushButton, QScrollArea
)
from PyQt6.QtCore import Qt, QPoint
from PyQt6.QtGui import QGuiApplication

from core import db

SCREEN_MARGIN = 8
MAX_ITEMS_HEIGHT = 200

class PopupWindow(QWidget):
    def __init__(self, on_unread_changed = None):
        super().__init__()
        self.on_unread_changed = on_unread_changed
        self.expanded_id = None
        self.anchor = (0, 0)
        
        self.setWindowFlags(Qt.WindowType.Popup | Qt.WindowType.FramelessWindowHint)
        self.setFixedWidth(300)
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
                widget.hide()
                widget.deleteLater()
        
        for channel_id, channel_title, description, color in db.get_all_channels():
            unread = db.get_unread_count(channel_id)
            is_open = channel_id == self.expanded_id
            arrow = "▴" if is_open else "▾"
            
            header = QPushButton(f"{channel_title}      {unread} unread {arrow}")
            header.setCursor(Qt.CursorShape.PointingHandCursor)
            header.setStyleSheet(
                f"text-align : left; color : white; padding : 8px; border : none; "
                f"border-left : 4px solid {color}; background-color : rgba(255, 255, 255, 15);"
            )
            header.clicked.connect(
                lambda checked = False, cid = channel_id : self.toggle_channel(cid)
            )
            self.layout_.addWidget(header)
            
            if is_open:
                self.layout_.addWidget(self.build_items_section(channel_id, unread))
    
    def build_items_section(self, channel_id, unread):
        section = QWidget()
        section.setStyleSheet("background : transparent;")
        section_layout = QVBoxLayout(section)
        section_layout.setContentsMargins(0, 0, 0, 0)
        section_layout.setSpacing(6)
        
        mark_btn = QPushButton("Mark all read")
        mark_btn.setEnabled(unread > 0)
        mark_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        mark_btn,self.setStyleSheet(
            "color : white; padding : 6px; border : 1px solid #555; border-radius : 4px; "
            "background-color : rgba(255, 255, 255, 10);"
        )
        mark_btn.clicked.connect(lambda checked = False : self.mark_read_clicked(channel_id))
        section_layout.addWidget(mark_btn)
        
        items = db.get_items(channel_id)
        
        inner = QWidget()
        inner.setStyleSheet("background : transparent;")
        inner_layout = QVBoxLayout(inner)
        inner_layout.setContentsMargins(0, 0, 0, 0)
        inner_layout.setSpacing(4)
        
        if not items:
            empty = QLabel("Nothing here yet.")
            empty.setStyleSheet("color : #888; padding : 6px;")
            inner_layout.addWidget(empty)
        
        for item_id, text, read in items:
            label = QLabel(text)
            label.setWordWrap(True)
            colour = "#888888" if read else "#ffffff"
            label.setStyleSheet(f"color : {colour}; padding : 4px 6px;")
            inner_layout.addWidget(label)
        
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setStyleSheet("QScrollArea {border : none; background : transparent;}")
        scroll.setWidget(inner)
        scroll.setFixedHeight(min(MAX_ITEMS_HEIGHT, 40 * max(1, len(items))))
        section_layout.addWidget(scroll)
        
        return section
    
    def toggle_channel(self, channel_id):
        self.expanded_id = None if self.expanded_id == channel_id else channel_id
        self.rebuild()
    
    def mark_read_clicked(self, channel_id):
        db.mark_all_read(channel_id)
        if self.on_unread_changed:
            self.on_unread_changed()
        self.rebuild()
    
    def rebuild(self):
        self.refresh()
        self.adjustSize()
        self.place()
    
    def place(self):
        x, y = self.anchor
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
    
    def show_near(self, x, y):
        self.anchor = (x, y)
        self.expanded_id = None
        self.rebuild()
        self.show()
        self.activateWindow()