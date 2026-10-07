import sys
import os
from pathlib import Path

def _is_frozen():
    return hasattr(sys, "_MEIPASS") #PyInstaller sets this in both one file and onedir modes

if _is_frozen():
    BASE_DIR = Path(sys._MEIPASS)
else:
    BASE_DIR = Path(__file__).parent.parent

_custom_data_dir = os.environ.get("WISPLY_DATA_DIR")
if _custom_data_dir:
    DATA_DIR = Path(_custom_data_dir)
else:
    DATA_DIR = Path(os.environ.get("APPDATA", str(Path.home()))) / "Wisply"
DATA_DIR.mkdir(parents = True, exist_ok = True)