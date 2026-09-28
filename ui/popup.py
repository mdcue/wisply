from PyQt6.QtWidgets import(
    QWidget, QVBoxLayout, QLabel, QPushButton, QScrollArea
)
from PyQt6.QtCore import Qt, QPoint, QTimer
from PyQt6.QtGui import QGuiApplication

from core import db

#layout info
POPUP_WIDTH = 300
SCREEN_MARGIN = 8
MAX_ITEMS_HEIGHT = 200
ITEM_ROW_ESTIMATE = 40

#colors
UNREAD_COLOUR = "#ffffff"
READ_COLOUR = "#888888"

#styles for parts of the GUI
TRANSPARENT = "background: transparent;"
POPUP_STYLE = "PopupWindow { background-color: #1e1e1e; border-radius: 8px; }"
TITLE_STYLE = "color: white; font-size: 14px; font-weight: bold;"
HEADER_STYLE = (
    "text-align: left; color: white; padding: 8px; border: none; "
    "border-left: 4px solid {color}; background-color: rgba(255, 255, 255, 15);"
)
MARK_BUTTON_STYLE = (
    "color: white; padding: 6px; border: 1px solid #555; border-radius: 4px; "
    "background-color: rgba(255, 255, 255, 10);"
)
ITEM_STYLE = "color: {colour}; padding: 4px 6px;"
EMPTY_STYLE = "color: #888; padding: 6px;"
SCROLL_STYLE = """
    QScrollArea { border: none; background: transparent; }
    QScrollBar:vertical { background: #2a2a2a; width: 10px; border-radius: 5px; margin: 0; }
    QScrollBar::handle:vertical { background: #888; border-radius: 5px; min-height: 24px; }
    QScrollBar::handle:vertical:hover { background: #bbb; }
    QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height: 0; }
    QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical { background: none; }
"""

#small helpers
def clamp(value, low, high):
    """Keep value between low and high"""
    return max(low, min(value, high))

def make_container(spacing):
    """A transparent widget with a zero-margin vertical layout. Returns (widget, layout)."""
    widget = QWidget()
    widget.setStyleSheet(TRANSPARENT)
    layout = QVBoxLayout(widget)
    layout.setContentsMargins(0, 0, 0, 0)
    layout.setSpacing(spacing)
    return widget, layout

class PopupWindow(QWidget):
    def __init__(self, on_unread_changed = None):
        super().__init__()
        self.on_unread_changed = on_unread_changed
        self.expanded_id = None
        self.anchor = (0, 0)
        
        self.setWindowFlags(Qt.WindowType.Popup | Qt.WindowType.FramelessWindowHint)
        self.setFixedWidth(POPUP_WIDTH)
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, True)
        self.setStyleSheet(POPUP_STYLE)
        
        self.layout_ = QVBoxLayout(self)
        self.layout_.setContentsMargins(12, 12, 12, 12)
        self.layout_.setSpacing(8)
        
        title = QLabel("wisply")
        title.setStyleSheet(TITLE_STYLE)
        self.layout_.addWidget(title)

    #For the UI
    def _clear_rows(self):
        while self.layout_.count() > 1:
            widget = self.layout_.takeAt(1).widget()
            if widget:
                widget.hide()
                widget.deleteLater()
    
    def refresh(self):
        self._clear_rows()
        for channel_id, title, description, color in db.get_all_channels():
            unread = db.get_unread_count(channel_id)
            is_open = channel_id == self.expanded_id
            self.layout_.addWidget(self._build_header(channel_id, title, unread, color, is_open))
            if is_open:
                self.layout_.addWidget(self._build_items_section(channel_id, unread))
    
    def _build_header(self, channel_id, title, unread, color, is_open):
        arrow = "▴" if is_open else "▾"
        header = QPushButton(f"{title}      {unread} unread {arrow}")
        header.setCursor(Qt.CursorShape.PointingHandCursor)
        header.setStyleSheet(HEADER_STYLE.format(color = color))
        header.clicked.connect(
            lambda _checked = False, cid = channel_id : self.toggle_channel(cid)
        )
        return header
    
    def _build_items_section(self, channel_id, unread):
        section, section_layout = make_container(spacing = 6)
        section_layout.addWidget(self._build_mark_button(channel_id, unread))
        section_layout.addWidget(self._build_items_list(db.get_items(channel_id)))
        return section
    
    def _build_mark_button(self, channel_id, unread):
        button = QPushButton("Mark all as read")
        button.setEnabled(unread > 0)
        button.setCursor(Qt.CursorShape.PointingHandCursor)
        button.setStyleSheet(MARK_BUTTON_STYLE)
        button.clicked.connect(lambda _checked = False: self.mark_read_clicked(channel_id))
        return button
    
    def _build_items_list(self, items):
        inner, inner_layout = make_container(spacing = 4)
        
        if not items:
            empty = QLabel("Nothing here yet.")
            empty.setStyleSheet(EMPTY_STYLE)
            inner_layout.addWidget(empty)
        
        for _item_id, text, read in items:
            label = QLabel(text)
            label.setWordWrap(True)
            label.setStyleSheet(ITEM_STYLE.format(colour = READ_COLOUR if read else UNREAD_COLOUR))
            inner_layout.addWidget(label)
        
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setStyleSheet(SCROLL_STYLE)
        scroll.setWidget(inner)
        scroll.setFixedHeight(min(MAX_ITEMS_HEIGHT, ITEM_ROW_ESTIMATE * max(1, len(items))))
        return scroll
    
    #for click inputs
    def toggle_channel(self, channel_id):
        self.expanded_id = None if self.expanded_id == channel_id else channel_id
        self.rebuild()
    
    def mark_read_clicked(self, channel_id):
        db.mark_all_read(channel_id)
        if self.on_unread_changed:
            self.on_unread_changed()
        self.rebuild()
    
    #position and sizing
    def rebuild(self):
        self.refresh()
        self.layout_.activate()
        self.resize(self.width(), self.layout_.sizeHint().height())
        self.place()
        QTimer.singleShot(0, self.place)
    
    def place(self):
        x, y = self.anchor
        screen = QGuiApplication.screenAt(QPoint(x, y)) or QGuiApplication.primaryScreen()
        area = screen.availableGeometry()
        
        min_x = area.x() + SCREEN_MARGIN
        max_x = area.x() + area.width() - self.width() - SCREEN_MARGIN
        min_y = area.y() + SCREEN_MARGIN
        max_y = area.y() + area.height() - self.height() - SCREEN_MARGIN
        
        self.move(clamp(x, min_x, max_x), clamp(y - self.height(), min_y, max_y))
    
    def show_near(self, x, y):
        self.anchor = (x, y)
        self.expanded_id = None
        self.rebuild()
        self.show()
        self.activateWindow()
