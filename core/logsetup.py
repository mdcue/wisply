import sys
import threading
from datetime import datetime

from core.paths import DATA_DIR

LOG_PATH = DATA_DIR / "wisply.log"
OLD_LOG_PATH = DATA_DIR / "wisply.log.old"
MAX_BYTES = 1_000_000 #limits the log file size to 1MB

class _LogFile:
    """One shared, thread-safe log file. Both stdout and stderr write through it,
    so lines from the scheduler thread, bridge threads, and the UI never interleave."""
    
    def __init__(self, file):
        self._file = file
        self._lock = threading.Lock()
        self._at_line_start = True
    
    def write(self, text):
        with self._lock:
            for chunk in text.splitlines(keepends = True):
                if self._at_line_start and chunk.strip():
                    self._file.write(datetime.now().strftime("%Y-%m-%d %H:%M:%S "))
                self._file.write(chunk)
                self._at_line_start = chunk.endswith("\n")
            self._file.flush()

class _Tee:
    """Replaces sys.stdout / sys.stderr: copies output to the log, and to the real
    console too if there is one (a packaged windowed app has none, so it is None)."""
    
    def __init__(self, console, log):
        self._console = console
        self._log = log
    
    def write(self, text):
        if self._console is not None:
            try:
                self._console.write(text)
            except Exception:
                pass
        self._log.write(text)
    
    def flush(self):
        if self._console is not None:
            try:
                self._console.flush()
            except Exception:
                pass
    
    def isatty(self):
        return False

def setup_logging():
    try:
        if LOG_PATH.exists() and LOG_PATH.stat().st_size > MAX_BYTES:
            LOG_PATH.replace(OLD_LOG_PATH)
        file = open(LOG_PATH, "a", encoding = "utf-8")
    except OSError:
        return
    
    log = _LogFile(file)
    sys.stdout = _Tee(sys.stdout, log)
    sys.stderr = _Tee(sys.stderr, log)
    print("---- Wisply started ----")