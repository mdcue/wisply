from PyQt6.QtCore import Qt, QDateTime, QTime
from PyQt6.QtWidgets import (
    QComboBox, QDateTimeEdit, QDialog, QDialogButtonBox, QFormLayout, QLineEdit
)

from bots.companion import MAX_TEXT, REPEATS

REPEAT_LABELS = {
    "none": "Doesn't repeat",
    "daily": "Every day",
    "weekly": "Every week",
    "monthly": "Every month",
    "yearly": "Every year",
}

class NoteDialog(QDialog):
    """A small window for jotting down a note: just the text for an idea, or the text, a date
    and time, and how often it repeats for a reminder."""
    def __init__(self, title, with_time):
        super().__init__()
        self.setWindowTitle(title)
        self.setMinimumWidth(380)
        
        form = QFormLayout(self)
        self.text = QLineEdit()
        self.text.setMaxLength(MAX_TEXT)
        self.text.setPlaceholderText("What should I remind you about?" if with_time else "Drop an idea in the jar")
        form.addRow("Note", self.text)
        
        self.when = None
        self.repeat = None
        if with_time:
            soon = QDateTime.currentDateTime().addSecs(3600)
            soon = QDateTime(soon.date(), QTime(soon.time().hour(), 0)) #the next full hour
            self.when = QDateTimeEdit(soon)
            self.when.setCalendarPopup(True)
            self.when.setDisplayFormat("ddd d MMM yyyy, HH:mm")
            form.addRow("When", self.when)
            self.repeat = QComboBox()
            for value in REPEATS:
                self.repeat.addItem(REPEAT_LABELS[value], value)
            form.addRow("Repeat", self.repeat)
        
        buttons = QDialogButtonBox(QDialogButtonBox.StandardButton.Ok | QDialogButtonBox.StandardButton.Cancel)
        buttons.accepted.connect(self.accept)
        buttons.rejected.connect(self.reject)
        form.addRow(buttons)
        
    def values(self):
        """(text, date, time, repeat), with the date and time in the form Companion expects."""
        if self.when is None:
            return self.text.text(), "", "", "none"
        stamp = self.when.dateTime()
        return self.text.text(), stamp.toString("yyyy-MM-dd"), stamp.toString("HH:mm"), self.repeat.currentData()

def ask_for_note(title, with_time):
    """Shows the window in front of everything else. Returns the values, or None if it was cancelled."""
    dialog = NoteDialog(title, with_time)
    dialog.setWindowFlag(Qt.WindowType.WindowStaysOnTopHint, True)
    dialog.show()
    dialog.raise_()
    dialog.activateWindow()
    dialog.text.setFocus()
    if dialog.exec() != QDialog.DialogCode.Accepted:
        return None
    return dialog.values()
