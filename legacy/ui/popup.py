import html
from pathlib import Path
from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QScrollArea, QMenu, QInputDialog, QColorDialog
)
from PyQt6.QtCore import Qt, QPoint, QTimer, QUrl, QSize, QEvent, pyqtSignal
from PyQt6.QtGui import QGuiApplication, QColor, QDesktopServices, QIcon

from core import db, appearance

POPUP_WIDTH = 300
SCREEN_MARGIN = 8
MAX_ITEMS_HEIGHT = 240
POPUP_MAX_ITEMS = 30
ITEM_ROW_ESTIMATES = 40
HEADER_ICON_SIZE = 28

UNREAD_COLOUR = "#ffffff"
READ_COLOUR = "#888888"

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
ITEM_HOVER_STYLE = "background-color: rgba(255, 255, 255, 48); border-radius: 4px;"
EMPTY_STYLE = "color: #888; padding: 6px;"
EMPTY_STATE_STYLE = "color: #888; padding: 12px; font-size: 13px;"
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

def clamp(value, low, high):
    return max(low, min(value, high))

def make_container(spacing):
    widget = QWidget()
    widget.setStyleSheet(TRANSPARENT)
    layout = QVBoxLayout(widget)
    layout.setContentsMargins(0, 0, 0, 0)
    layout.setSpacing(spacing)
    return widget, layout

class ItemLabel(QLabel):
    """A QLabel that highlights on hover. Both looks are in its stylesheet from the start, so
    moving the mouse never restyles it. An item with a link handles its own click; one that is
    created with clickable = True (an unread item with nothing to open) sends `clicked`."""
    clicked = pyqtSignal()
    
    def __init__(self, base_style, clickable = False):
        super().__init__()
        self.clickable = clickable
        if clickable:
            self.setCursor(Qt.CursorShape.PointingHandCursor)
        self.setAttribute(Qt.WidgetAttribute.WA_Hover, True)
        self.setStyleSheet(f"ItemLabel {{ {base_style} }} ItemLabel:hover {{ {ITEM_HOVER_STYLE} }}")

    def mouseReleaseEvent(self, event):
        if self.clickable and event.button() == Qt.MouseButton.LeftButton and self.rect().contains(event.position().toPoint()):
            self.clicked.emit()
        super().mouseReleaseEvent(event)

class ChannelBlock(QWidget):
    """One channel of a wisp inside the popup: a header you click to open it, and its list of
    items. The list is built the first time the channel is opened and kept, so switching
    channels only shows and hides widgets instead of rebuilding the whole popup."""
    def __init__(self, popup, channel_id, title, color, unread, items, list_height):
        super().__init__()
        self.popup = popup
        self.channel_id = channel_id
        self.title = title
        self.unread = unread
        self.items = items
        self.list_height = list_height
        self.items_widget = None
        self.setStyleSheet(TRANSPARENT)
        
        self.layout_ = QVBoxLayout(self)
        self.layout_.setContentsMargins(0, 0, 0, 0)
        self.layout_.setSpacing(4)
        
        top = QWidget()
        top.setStyleSheet(TRANSPARENT)
        top_layout = QHBoxLayout(top)
        top_layout.setContentsMargins(0, 0, 0, 0)
        
        self.header = QPushButton()
        self.header.setCursor(Qt.CursorShape.PointingHandCursor)
        self.header.setStyleSheet(CHANNEL_LABEL_STYLE.format(color = color) + "text-align: left; border: none; padding: 2px 0; background: transparent;")
        self.header.clicked.connect(lambda _c = False: popup.open_channel(channel_id))
        top_layout.addWidget(self.header)
        top_layout.addStretch()
        self.mark_button = popup._build_mark_button(channel_id, unread)
        keep_space = self.mark_button.sizePolicy()
        keep_space.setRetainSizeWhenHidden(True)
        self.mark_button.setSizePolicy(keep_space)
        top_layout.addWidget(self.mark_button)
        self.layout_.addWidget(top)
        
        self.set_open(False)

    def set_open(self, is_open):
        arrow = "▾" if is_open else "▸"
        self.header.setText(f"{arrow} {self.title}  .  {self.unread} unread")
        self.mark_button.setVisible(is_open)
        if is_open and self.items_widget is None:
            self.items_widget = self.popup._build_items_list(self.channel_id, self.items, self.list_height)
            self.layout_.addWidget(self.items_widget)
        if self.items_widget is not None:
            self.items_widget.setVisible(is_open)

class PopupWindow(QWidget):
    def __init__(self, on_unread_changed = None, bot_id = None, popup_style = True):
        super().__init__()
        self.on_unread_changed = on_unread_changed
        self.bot_id_filter = bot_id
        self.popup_style = popup_style
        self.expanded_id = None
        self.expanded_channel = None
        self.channel_blocks = {}
        self.anchor = (0, 0)
        
        if self.popup_style:
            self.setWindowFlags(Qt.WindowType.Popup | Qt.WindowType.FramelessWindowHint)
        else:
            self.setWindowFlags(Qt.WindowType.Tool | Qt.WindowType.FramelessWindowHint)
        self.setFixedWidth(POPUP_WIDTH)
        self.setAttribute(Qt.WidgetAttribute.WA_StyledBackground, True)
        self.setStyleSheet(POPUP_STYLE)
        
        self.layout_ = QVBoxLayout(self)
        self.layout_.setContentsMargins(12, 12, 12, 12)
        self.layout_.setSpacing(8)
        
        title = QLabel("wisply")
        title.setStyleSheet(TITLE_STYLE)
        self.layout_.addWidget(title)
        
    def changeEvent(self, event):
        if event.type() == QEvent.Type.ActivationChange:
            if not self.popup_style and not self.isActiveWindow():
                self.hide()
        super().changeEvent(event)
    
    def _clear_rows(self):
        while self.layout_.count() > 1:
            widget = self.layout_.takeAt(1).widget()
            if widget:
                widget.hide()
                widget.deleteLater()
    
    def _bots_to_show(self):
        if self.bot_id_filter:
            bot = db.get_bot(self.bot_id_filter)
            return [bot] if bot and bot[3] else [] #bot[3] refers to "active" - still does hide it if somehow paused
        return db.get_active_bots() #hides inactive bots
    
    def refresh(self):
        self._clear_rows()
        self.channel_blocks = {}
        bots = self._bots_to_show()
        
        if not bots:
            empty = QLabel("No active wisps yet. \nOpen the Hub to activate one.")
            empty.setStyleSheet(EMPTY_STATE_STYLE)
            self.layout_.addWidget(empty)
            return
        
        for bot_id, name, color, active, icon_path, expression, fetch_interval, last_fetched in bots: #unpacking all info
            unread = db.get_bot_unread(bot_id)
            is_open = bot_id == self.expanded_id or self.bot_id_filter == bot_id
            self.layout_.addWidget(self._build_header(bot_id, name, color, icon_path, unread, is_open))
            if is_open:
                self.layout_.addWidget(self._build_bot_section(bot_id))
    
    def _build_header(self, bot_id, name, color, icon_path, unread, is_open):
        arrow = "▴" if is_open else "▾"
        header = QPushButton(f" {name}  {unread} unread {arrow}")
        header.setCursor(Qt.CursorShape.PointingHandCursor)
        header.setStyleSheet(HEADER_STYLE.format(color = color))
        
        if icon_path and Path(icon_path).exists():
            header.setIcon(QIcon(icon_path))
            header.setIconSize(QSize(HEADER_ICON_SIZE, HEADER_ICON_SIZE))
        
        if not self.bot_id_filter:
            header.clicked.connect(lambda _c = False, bid = bot_id: self.toggle_bot(bid))
        header.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        header.customContextMenuRequested.connect(
            lambda pos: self._show_header_menu(bot_id, header.mapToGlobal(pos))
        )
        return header
    
    def _build_bot_section(self, bot_id):
        section, section_layout = make_container(spacing = 6)
        channels = db.get_bot_channels(bot_id)
        items_by_channel = {channel[0]: db.get_items(channel[0], limit = POPUP_MAX_ITEMS) for channel in channels}
        unread_by_channel = {channel[0]: db.get_unread_count(channel[0]) for channel in channels}
        open_id = self._channel_to_open(unread_by_channel)
        self.expanded_channel = open_id #stays open if the popup is rebuilt, e.g. after "Mark all read"
        
        #Every channel's list gets the height of the tallest, so switching channels never changes the window's size
        tallest = max((len(items) for items in items_by_channel.values()), default = 1)
        list_height = min(MAX_ITEMS_HEIGHT, ITEM_ROW_ESTIMATES * max(1, tallest))
        
        for channel_id, title, _desc, color in channels:
            block = ChannelBlock(self, channel_id, title, color, unread_by_channel[channel_id], items_by_channel[channel_id], list_height)
            block.set_open(channel_id == open_id)
            self.channel_blocks[channel_id] = block
            section_layout.addWidget(block)
        return section
    
    def _channel_to_open(self, unread_by_channel):
        """Only one channel of a wisp is open at a time, so a wisp with many channels stays
        short. Defaults to the first channel that has something unread."""
        if self.expanded_channel in unread_by_channel:
            return self.expanded_channel
        for channel_id, unread in unread_by_channel.items():
            if unread > 0:
                return channel_id
        return next(iter(unread_by_channel), None)
    
    def _build_mark_button(self, channel_id, unread):
        button = QPushButton("Mark all read")
        button.setEnabled(unread > 0)
        button.setCursor(Qt.CursorShape.PointingHandCursor)
        button.setStyleSheet(MARK_BUTTON_STYLE)
        button.clicked.connect(lambda _c = False: self.mark_read_clicked(channel_id))
        return button
    
    def _build_items_list(self, channel_id, items, height = None):
        inner, inner_layout = make_container(spacing = 4)
        
        if not items:
            empty = QLabel("Nothing here yet.")
            empty.setStyleSheet(EMPTY_STYLE)
            inner_layout.addWidget(empty)
        
        for item_id, text, read, url, price, discount in items:
            colour = READ_COLOUR if read else UNREAD_COLOUR
            safe_text = html.escape(text)
            
            if url:
                base_style = "padding: 4px 6px; "
                label = ItemLabel(base_style)
                label.setWordWrap(True)
                label.setTextFormat(Qt.TextFormat.RichText)
                label.setText(f'<a href="{html.escape(url, quote = True)}" style="color:{colour}; text-decoration:none;">{safe_text}</a>')
                label.setCursor(Qt.CursorShape.PointingHandCursor)
                label.linkActivated.connect(
                    lambda _url, iid = item_id, cid = channel_id: self._item_clicked(iid, cid, _url)
                )
            else:
                base_style = ITEM_STYLE.format(colour = colour)
                label = ItemLabel(base_style, clickable = not read)
                label.setWordWrap(True)
                label.setText(safe_text)
                if not read:
                    label.setToolTip("Click to mark as read")
                    label.clicked.connect(lambda iid = item_id: self._item_read(iid))
            inner_layout.addWidget(label)
            
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setStyleSheet(SCROLL_STYLE)
        scroll.setWidget(inner)
        scroll.setFixedHeight(height or min(MAX_ITEMS_HEIGHT, ITEM_ROW_ESTIMATES * max(1, len(items))))
        return scroll
    
    def open_channel(self, channel_id):
        """Switches which channel is open. Nothing is rebuilt: lists are shown and hidden in place."""
        if channel_id not in self.channel_blocks:
            return
        self.expanded_channel = channel_id
        self.setUpdatesEnabled(False)
        try:
            for block_id, block in self.channel_blocks.items():
                block.set_open(block_id == channel_id)
            self._relayout()
            self.fit()
        finally:
            self.setUpdatesEnabled(True)
    
    def _relayout(self):
        """Recomputes every layout between the channel blocks and the window right now, so a
        show or hide is reflected in the window's height within the same step."""
        for block in self.channel_blocks.values():
            widget = block
            while widget is not None and widget is not self:
                if widget.layout():
                    widget.layout().invalidate()
                widget.updateGeometry() #drops the size its parent layout remembered for this widget
                widget = widget.parentWidget()
        self.layout_.invalidate()
        self.layout_.activate()
    
    def toggle_bot(self, bot_id):
        self.expanded_id = None if self.expanded_id == bot_id else bot_id
        self.rebuild()
    
    def _item_clicked(self, item_id, channel_id,url):
        db.mark_read(item_id)
        QDesktopServices.openUrl(QUrl(url))
        if self.on_unread_changed:
            self.on_unread_changed()
        self.rebuild()
    
    def _item_read(self, item_id):
        db.mark_read(item_id)
        if self.on_unread_changed:
            self.on_unread_changed()
        self.rebuild()
    
    def mark_read_clicked(self, channel_id):
        db.mark_all_read(channel_id)
        if self.on_unread_changed:
            self.on_unread_changed()
        self.rebuild()
    
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
        new_name, ok = QInputDialog.getText(None, "Rename wisp", "New name:", text = bot[1])
        new_name = new_name.strip()
        if ok and new_name:
            db.update_bot(bot_id, name = new_name)
        self._reopen()
    
    def _recolor_bot(self, bot_id):
        bot = db.get_bot(bot_id)
        if not bot:
            return
        chosen = QColorDialog.getColor(QColor(bot[2]), None, "Choose wisp color")
        if chosen.isValid():
            appearance.set_bot_appearance(bot_id, color = chosen.name())
        self._reopen()
    
    def _reopen(self):
        self.rebuild()
        self.show()
        self.activateWindow()
    
    def rebuild(self):
        #Painting is paused while the rows are swapped, so the window only ever draws the finished result
        self.setUpdatesEnabled(False)
        try:
            self.refresh()
            self.layout_.activate()
            self.fit()
        finally:
            self.setUpdatesEnabled(True)
        QTimer.singleShot(0, self.fit) #this is harmless when nothing changed"; corrects the size if Qt adjusted it after showing
    
    def fit(self):
        """Sets the window's size and position together, in on step, so it never shows an in-between state."""
        x, y = self.anchor
        height = self.layout_.sizeHint().height()
        screen = QGuiApplication.screenAt(QPoint(x, y)) or QGuiApplication.primaryScreen()
        area = screen.availableGeometry()
        
        min_x = area.x() + SCREEN_MARGIN
        max_x = area.x() + area.width() - self.width() - SCREEN_MARGIN
        min_y = area.y() + SCREEN_MARGIN
        max_y = area.y() + area.height() - height - SCREEN_MARGIN
        
        self.setGeometry(clamp(x, min_x, max_x), clamp(y - height, min_y, max_y), self.width(), height)
    
    def show_near(self, x, y):
        self.anchor = (x, y)
        self.expanded_id = None
        self.expanded_channel = None
        self.rebuild()
        self.show()
        self.activateWindow()
