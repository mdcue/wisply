from PyQt6.QtCore import QObject, pyqtSignal
from PyQt6.QtNetwork import QLocalServer, QLocalSocket

SERVER_NAME = "wisply-single-instance"

class SingleInstance(QObject):
    """Makes sure only one Wisply runs. The running copy listens on a local name pipe;
    a second copy connects to it, optionally asks it to show its Hub, and exists."""
    
    show_requested = pyqtSignal()
    
    def __init__(self):
        super().__init__()
        self._server = None
    
    def notify_existing(self, show):
        """Returns True if another Wisply is already running (and tells it to show its
        Hub when show=True). Returns False if we are the first copy."""
        socket = QLocalSocket()
        socket.connectToServer(SERVER_NAME)
        if not socket.waitForConnected(500):
            return False
        if show:
            socket.write(b"show")
            socket.waitForBytesWritten(500)
        socket.disconnectFromServer()
        return True

    def start_listening(self):
        """Claims the name so later launches can find this copy. Returns True on success."""
        QLocalServer.removeServer(SERVER_NAME) #this will clear a leftover name on platforms that have one
        self._server = QLocalServer(self)
        self._server.newConnection.connect(self._on_connection)
        return self._server.listen(SERVER_NAME)
    
    def _on_connection(self):
        connection = self._server.nextPendingConnection()
        if connection is None:
            return
        connection.readyRead.connect(lambda c = connection: self._on_ready(c))
        connection.disconnected.connect(connection.deleteLater)
    
    def _on_ready(self, connection):
        if bytes(connection.readAll()).strip() == b"show":
            self.show_requested.emit()
        connection.disconnectFromServer()