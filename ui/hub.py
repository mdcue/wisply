import sys
from pathlib import Path
from PyQt6.QtWidgets import (
    QWidget, QVBoxLayout, QHBoxLayout, QLabel, QPushButton,
    QScrollArea, QMenu, QInputDialog, QColorDialog, QApplication
)
from PyQt6.QtCore import Qt, QSize
from PyQt6.QtGui import QIcon, QColor

from core import db, appearance, icons
from bots.registry import ALL_BOTS, BOT_MODULES

WINDOW_WIDTH = 380
WINDOW_HEIGHT = 560
ROW_ICON_SIZE = 40

UNREAD_COLOUR = "#ffffff"
READ_COLOUR = "#888888"

WINDOW_STYLE = "background-color: #1e1e1e;"
TITLE_STYLE = "color: white; font-size: 16px; font-weight: bold; padding: 4px 0 12px 0;"
ROW_STYLE = (
    "text-align: left; color: white; padding: 10px; border: none; border-radius: 6px; "
    "border-left: 4px solid {color}; background-color: rgba(255, 255, 255, 15);"
)
INACTIVE_ROW_STYLE = (
    "text-align: left; color: #999; padding: 10px; border: none; border-radius: 6px; "
    "border-left: 4px solid #555; background-color: rgba(255, 255, 255, 6);"
)
TOGGLE_ON_STYLE = "color: white; padding: 4px 10px; border-radius: 4px; background-color: #3a7d3a;"
TOGGLE_OFF_STYLE = "color: #ccc; padding: 4px 10px; border-radius: 4px; background-color: #555;"
CHANNEL_LABEL_STYLE = "color: {color}; font-size: 12px; font-weight: bold;"
MARK_BUTTON_STYLE = (
    "color: white; padding: 3px 8px; border: 1px solid #555; border-radius: 4px; "
    "background-color: rgba(255, 255, 255, 10);"
)
ITEM_STYLE = "color: {colour}; padding: 4px 6px;"
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

def make_container(spacing):
    widget = QWidget()
    widget.setStyleSheet("background: transparent;")
    layout = QVBoxLayout(widget)
    layout.setContentsMargins(0, 0, 0, 0)
    layout.setSpacing(spacing)
    return widget, layout

class HubWindow(QWidget):
    """An ordinary window listing every bot (active or not), with per-bot
    activate/deactivate, rename, recolor, and expression, plus an expandable
    channel/item view for active bots. Build independently of popup.py."""
    
    def __init__(self, on_unread_changed = None):
        super().__init__()
        self.on_unread_changed = on_unread_changed
        self.expanded_id = None
        
        self.setWindowTitle("wisply Hub")
        self.resize(WINDOW_WIDTH, WINDOW_HEIGHT)
        self.setStyleSheet(WINDOW_STYLE)
        
        outer = QVBoxLayout(self)
        outer.setContentsMargins(16, 16, 16, 16)
        outer.setSpacing(10)
        
        title = QLabel("wisply Hub")
        title.setStyleSheet(TITLE_STYLE)
        outer.addWidget(title)
        
        self.scroll = QScrollArea()
        self.scroll.setWidgetResizable(True)
        self.scroll.setStyleSheet(SCROLL_STYLE)
        outer.addWidget(self.scroll)
        
        self.list_widget, self.list_layout = make_container(spacing = 8)
        self.scroll.setWidget(self.list_widget)
        
        self.refresh()
    
    def refresh(self):
        while self.list_layout.count():
            widget = self.list_layout.takeAt(0).widget()
            if widget:
                widget.hide()
                widget.deleteLater()
        
        bots = db.get_all_bots()
        if not bots:
            empty = QLabel("No bots registered yet.")
            empty.setStyleSheet(EMPTY_STYLE)
            self.list_layout.addWidget(empty)
            return
        
        for bot_id, name, color, active, icon_path, expression in bots:
            self.list_layout.addWidget(self._build_bot_row(bot_id, name, color, active, icon_path))
            if active and bot_id == self.expanded_id:
                self.list_layout.addWidget(self._build_bot_section(bot_id))
        
        self.list_layout.addStretch()
    
    def _build_bot_row(self, bot_id, name, color, active, icon_path):
        row, row_layout = make_container(spacing = 6)
        row_layout.setContentsMargins(0, 0, 0, 0)
        top = QWidget()
        top.setStyleSheet("background: transparent;")
        top_layout = QHBoxLayout(top)
        top_layout.setContentsMargins(0, 0, 0, 0)
        
        arrow = ""
        if active:
            arrow = "  ▴" if bot_id == self.expanded_id else "  ▾"
        
        name_btn = QPushButton(f"  {name}{arrow}")
        name_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        name_btn.setStyleSheet((ROW_STYLE if active else INACTIVE_ROW_STYLE).format(color = color))
        if icon_path and Path(icon_path).exists():
            name_btn.setIcon(QIcon(icon_path))
            name_btn.setIconSize(QSize(ROW_ICON_SIZE, ROW_ICON_SIZE))
        if active:
            name_btn.clicked.connect(lambda _c = False, bid = bot_id: self._toggle_expand(bid))
        name_btn.setContextMenuPolicy(Qt.ContextMenuPolicy.CustomContextMenu)
        name_btn.customContextMenuRequested.connect(
            lambda pos, bid = bot_id: self._show_bot_menu(bid, name_btn.mapToGlobal(pos))
        )
        toggle_btn = QPushButton("On" if active else "Off")
        toggle_btn.setFixedWidth(50)
        toggle_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        toggle_btn.setStyleSheet(TOGGLE_ON_STYLE if active else TOGGLE_OFF_STYLE)
        toggle_btn.clicked.connect(lambda _c = False, bid = bot_id, cur = active: self._toggle_active(bid, cur))
        
        top_layout.addWidget(name_btn, stretch = 1)
        top_layout.addWidget(toggle_btn)
        row_layout.addWidget(top)
        return row
    
    def _build_bot_section(self, bot_id):
        section, section_layout = make_container(spacing = 8)
        for channel_id, title, _desc, color in db.get_bot_channels(bot_id):
            section_layout.addWidget(self._build_channel_block(channel_id, title, color))
        return section
    
    def _build_channel_block(self, channel_id, title, color):
        block, block_layout = make_container(spacing = 4)
        unread = db.get_unread_count(channel_id)
        
        top = QWidget()
        top.setStyleSheet("background: transparent;")
        top_layout = QHBoxLayout(top)
        top_layout.setContentsMargins(0, 0, 0, 0)
        
        label = QLabel(f"{title}  .  {unread} unread")
        label.setStyleSheet(CHANNEL_LABEL_STYLE.format(color = color))
        top_layout.addWidget(label)
        top_layout.addStretch()
        
        mark_btn = QPushButton("Mark all read")
        mark_btn.setEnabled(unread > 0)
        mark_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        mark_btn.setStyleSheet(MARK_BUTTON_STYLE)
        mark_btn.clicked.connect(lambda _c = False: self._mark_read_clicked(channel_id))
        top_layout.addWidget(mark_btn)
        
        block_layout.addWidget(top)
        
        items = db.get_items(channel_id)
        inner, inner_layout = make_container(spacing = 4)
        if not items:
            empty = QLabel("Nothing here yet.")
            empty.setStyleSheet(EMPTY_STYLE)
            inner_layout.addWidget(empty)
        for item_id, text, read, url in items[:10]:
            colour = READ_COLOUR if read else UNREAD_COLOUR
            label = QLabel()
            label.setWordWrap(True)
            if url:
                label.setTextFormat(Qt.TextFormat.RichText)
                label.setText(f'<a href="{url}" style="color:{colour}; text-decoration:none;">{text}</a>')
                label.setOpenExternalLinks(True)
                label.setStyleSheet("padding: 4px 6px;")
            else:
                label.setText(text)
                label.setStyleSheet(ITEM_STYLE.format(colour = colour))
            inner_layout.addWidget(inner)
        block_layout.addWidget(inner)
        return block
    
    def _toggle_expand(self, bot_id):
        self.expanded_id = None if self.expanded_id == bot_id else bot_id
        self.refresh()
    
    def _toggle_active(self, bot_id, currently_active):
        new_state = not currently_active
        db.set_bot_active(bot_id, new_state)
        if new_state:
            bot_module = BOT_MODULES.get(bot_id)
            if bot_module:
                bot_module.fetch() #fetch immediately on activation, per the earlier design decision
        else:
            self.expanded_id = None if self.expanded_id == bot_id else self.expanded_id
        if self.on_unread_changed:
            self.on_unread_changed()
        self.refresh()
    
    def _mark_read_clicked(self, channel_id):
        db.mark_all_read(channel_id)
        if self.on_unread_changed:
            self.on_unread_changed()
        self.refresh()
    
    def _show_bot_menu(self, bot_id, global_pos):
        menu = QMenu(self)
        menu.setStyleSheet(MENU_STYLE)
        rename_action = menu.addAction("Rename...")
        color_action = menu.addAction("Change color...")
        expression_action = menu.addAction("Change expression...")
        
        chosen = menu.exec(global_pos)
        if chosen is rename_action:
            self._rename_bot(bot_id)
        elif chosen is color_action:
            self._recolor_bot(bot_id)
        elif chosen is expression_action:
            self._change_expression(bot_id)
    
    def _rename_bot(self, bot_id):
        bot = db.get_bot(bot_id)
        if not bot:
            return
        new_name, ok = QInputDialog.getText(None, "Rename bot", "New name:", text = bot[1])
        new_name = new_name.strip()
        if ok and new_name:
            db.update_bot(bot_id, name = new_name)
        self.refresh()
    
    def _recolor_bot(self, bot_id):
        bot = db.get_bot(bot_id)
        if not bot:
            return
        chosen = QColorDialog.getColor(QColor(bot[2]), None, "Choose bot color")
        if chosen.isValid():
            appearance.set_bot_appearance(bot_id, color = chosen.name()) #regenerates the icon files too
        self.refresh()
    
    def _change_expression(self, bot_id):
        bot = db.get_bot(bot_id)
        if not bot:
            return
        options = icons.get_available_expressions()
        current = bot[5] if bot[5] in options else options[0]
        choice, ok = QInputDialog.getItem(
            None, "Change expression", "Pick an expression:", options,
            options.index(current), editable = False
        )
        if ok and choice:
            appearance.set_bot_appearance(bot_id, expression = choice)
        self.refresh()

if __name__ == "__main__":
    #visual test
    app = QApplication(sys.argv)
    db.init_db()
    for bot in ALL_BOTS:
        bot.register()
    
    hub = HubWindow()
    hub.show()
    
    sys.exit(app.exec())
