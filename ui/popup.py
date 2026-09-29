import html
from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QScrollArea, QMenu, QInputDialog, QColorDialog
)
from PyQt6.QtCore import Qt, QPoint, QTimer, QUrl
from PyQt6.QtGui import QGuiApplication, QColor, QDesktopServices

from core import db

#layout info
POPUP_WIDTH = 300
SCREEN_MARGIN = 8
MAX_ITEMS_HEIGHT = 150
ITEM_ROW_ESTIMATE = 40

#colors
UNREAD_COLOUR = "#ffffff"
READ_COLOUR = "#888888"

#stylizing for the GUI
TRANSPARENT = "background: transparent;"
POPUP_STYLE = "PopupWindow { background-color: #1e1e1e; border-radius: 8px; }"
TITLE_STYLE = "color: white; font-size: 14px; font-weight: bold;"
HEADER_STYLE = (
    "text-align: left; color: white; padding: 8px; border: none; "
    "border-left: 4px solid {color}; background-color: rgba(255, 255, 255, 15);"
)

CHANNEL_LABEL_STYLE = "color: {color}; font-size: 12px; font-weight: bold;"
MARK_BUTTON_STYLE = (
    "color: white; padding: 3px 8px; border: 1px solid #555; border-radius: 4px; "
    "background-color: rgba(255, 255, 255, 10);"
)

ITEM_STYLE = "color: {colour}; padding: 4px 6px;"
ITEM_HOVER_STYLE = "background-color: rgba(255, 255, 255, 25); border-radius: 4px;"
EMPTY_STYLE = "color: #888; padding: 6px;"
MENU_STYLE = (
    "QMenu { background-color: #2a2a2a; color: white; border: 1px solid #555; }"
    "QMenu::item { padding: 6px 20px; }"
    "QMenu::item:selected { background-color: #444; }"
)

SCROLL_STYLE = """
    QScrollArea { border: none; background: transparent; }
    QScrollBar:vertical { background: #2a2a2a; width: 10px; border-radius: 5px; margin: 0; }
    QScrollBar::handle:vertical { background: #888; border-radius: 5px; min-height: 24px; }
    QScrollBar::handle:vertical:hover { background: #bbb; }
    QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height: 0; }
    QScrollBar::add-page:vertical, QScrollBar::sub-page:vertical { background: none; }
"""

#some small helpers
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

class ItemLabel(QLabel):
    """A QLabel that highlights on hover and (if it has a url) marks itself read on click."""
    def __init__(self, base_style):
        super().__init__()
        self.base_style = base_style
        self.setStyleSheet(self.base_style)
    
    def enterEvent(self, event):
        self.setStyleSheet(self.base_style + ITEM_HOVER_STYLE)
        super().enterEvent(event)
    
    def leaveEvent(self, event):
        self.setStyleSheet(self.base_style)
        super().leaveEvent(event)

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
    
    #for the UI
    def _clear_rows(self):
        while self.layout_.count() > 1:
            widget = self.layout_.takeAt(1).widget()
            if widget:
                widget.hide()
                widget.deleteLater()
    
    def refresh(self):
        self._clear_rows()
        for bot_id, name, color in db.get_all_bots():
            unread = db.get_bot_unread(bot_id)
            is_open = bot_id == self.expanded_id
            self.layout_.addWidget(self._build_header(bot_id, name, unread, color, is_open))
            if is_open:
                self.layout_.addWidget(self._build_bot_section(bot_id))
    
    def _build_header(self, bot_id, name, unread, color, is_open):
        arrow = "▴" if is_open else "▾"
        header = QPushButton(f"{name}   {unread} unread {arrow}")
        header.setCursor(Qt.CursorShape.PointingHandCursor)
        header.setStyleSheet(HEADER_STYLE.format(color = color))
        header.clicked.connect(
            lambda _checked = False, bid = bot_id : self.toggle_bot(bid)
        )
        header.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        header.customContextMenuRequested.connect(
            lambda pos: self._show_header_menu(bot_id, header.mapToGlobal(pos))
        )
        return header
    
    def _build_bot_section(self, bot_id):
        section, section_layout = make_container(spacing = 8)
        for channel_id, title, _description, color in db.get_bot_channels(bot_id):
            section_layout.addWidget(self._build_channel_block(channel_id, title, color))
        return section
    
    def _build_channel_block(self, channel_id, title, color):
        block, block_layout = make_container(spacing = 4)
        unread = db.get_unread_count(channel_id)
        
        top = QWidget()
        top.setStyleSheet(TRANSPARENT)
        top_layout = QHBoxLayout(top)
        top_layout.setContentsMargins(0, 0, 0, 0)
        
        label = QLabel(f"{title}  .  {unread} unread")
        label.setStyleSheet(CHANNEL_LABEL_STYLE.format(color = color))
        top_layout.addWidget(label)
        top_layout.addStretch()
        top_layout.addWidget(self._build_mark_button(channel_id, unread))
        
        block_layout.addWidget(top)
        block_layout.addWidget(self._build_items_list(channel_id, db.get_items(channel_id)))
        return block
    
    def _build_mark_button(self, channel_id, unread):
        button = QPushButton("Mark all read")
        button.setEnabled(unread > 0)
        button.setCursor(Qt.CursorShape.PointingHandCursor)
        button.setStyleSheet(MARK_BUTTON_STYLE)
        button.clicked.connect(
            lambda _checked = False: self.mark_read_clicked(channel_id)
        )
        return button
    
    def _build_items_list(self, channel_id, items):
        inner, inner_layout = make_container(spacing = 4)
        
        if not items:
            empty = QLabel("Nothing here yet.")
            empty.setStyleSheet(EMPTY_STYLE)
            inner_layout.addWidget(empty)
        
        for item_id, text, read, url in items:
            colour = READ_COLOUR if read else UNREAD_COLOUR
            safe_text = html.escape(text)
            
            if url:
                base_style = "padding: 4px 6px;"
                label = ItemLabel(base_style)
                label.setWordWrap(True)
                label.setTextFormat(Qt.TextFormat.RichText)
                label.setText(f'<a href="{url}" style = "color:{colour}; text-decoration:none;">{safe_text}</a>')
                label.setCursor(Qt.CursorShape.PointingHandCursor)
                label.linkActivated.connect(
                    lambda _url, iid =  item_id, cid = channel_id: self._item_clicked(iid, cid, _url)
                )
            else:
                base_style = ITEM_STYLE.format(colour = colour)
                label = ItemLabel(base_style)
                label.setWordWrap(True)
                label.setText(safe_text)
            
            inner_layout.addWidget(label)
        
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setStyleSheet(SCROLL_STYLE)
        scroll.setWidget(inner)
        scroll.setFixedHeight(min(MAX_ITEMS_HEIGHT, ITEM_ROW_ESTIMATE * max(1, len(items))))
        return scroll
    
    #for click inputs
    def toggle_bot(self, bot_id):
        self.expanded_id = None if self.expanded_id == bot_id else bot_id
        self.rebuild()
    
    def _item_clicked(self, item_id, channel_id, url):
        db.mark_read(item_id)
        QDesktopServices.openUrl(QUrl(url))
        if self.on_unread_changed:
            self.on_unread_changed()
        self.rebuild()
    
    def mark_read_clicked(self, channel_id):
        db.mark_all_read(channel_id)
        if self.on_unread_changed:
            self.on_unread_changed()
        self.rebuild()
    
    #renaming and recoloring
    def _show_header_menu(self, bot_id, global_pos):
        menu = QMenu(self)
        menu.setStyleSheet(MENU_STYLE)
        rename_action = menu.addAction("Rename...")
        color_action = menu.addAction("Change color...")
        
        chosen = menu.exec(global_pos)
        if chosen is rename_action:
            self._rename_bot(bot_id)
        elif chosen is color_action:
            self._recolor_bot(bot_id)
    
    def _rename_bot(self, bot_id):
        bot = db.get_bot(bot_id)
        if not bot:
            return
        new_name, ok = QInputDialog.getText(None, "Rename bot", "New name:", text = bot[1])
        new_name = new_name.strip()
        if ok and new_name:
            db.update_bot(bot_id, name = new_name)
        self._reopen()
    
    def _recolor_bot(self, bot_id):
        bot = db.get_bot(bot_id)
        if not bot:
            return
        chosen = QColorDialog.getColor(QColor(bot[2]), None, "Choose bot color")
        if chosen.isValid():
            db.update_bot(bot_id, color = chosen.name())
        self._reopen()
    
    def _reopen(self):
        #the popup can close when a dialog takes focus, so redraw and bring it back
        self.rebuild()
        self.show()
        self.activateWindow()
    
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
