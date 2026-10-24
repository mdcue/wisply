import sys
import os
from pathlib import Path

def _is_frozen():
    return hasattr(sys, "_MEIPASS") #PyInstaller sets this in both one file and onedir modes

if _is_frozen():
    BASE_DIR = Path(sys._MEIPASS)
else:
    BASE_DIR = Path(__file__).parent.parent

def _default_data_dir():
    return Path(os.environ.get("APPDATA", str(Path.home()))) / "Wisply"

def _resolve_data_dir():
    custom = os.environ.get("WISPLY_DATA_DIR")
    if custom:
        try:
            path = Path(custom)
            path.mkdir(parents = True, exist_ok = True)
            return path
        except OSError:
            print(f"WISPLY_DATA_DIR ({custom}) is not usable, using the default location instead.")
    
    path = _default_data_dir()
    path.mkdir(parents = True, exist_ok = True)
    return path

DATA_DIR = _resolve_data_dir()