import sys, winreg
from pathlib import Path

APP_NAME = "wisply"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"

def _get_command():
    if getattr(sys, "frozen", False):
        #packaged build: sys.executable is the Wisply.exe itself
        return f'"{sys.executable}" --background'
    #Dev mode: run main.py with the current python
    main_path = Path(__file__).parent.parent / "main.py"
    return f'"{sys.executable}" "{main_path}" --background'

def is_enabled():
    try:
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_READ)
        winreg.QueryValueEx(key, APP_NAME)
        winreg.CloseKey(key)
        return True
    except FileNotFoundError:
        return False

def enable():
    key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE)
    winreg.SetValueEx(key, APP_NAME, 0, winreg.REG_SZ, _get_command())
    winreg.CloseKey(key)

def disable():
    try:
        key = winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE)
        winreg.DeleteValue(key, APP_NAME)
        winreg.CloseKey(key)
    except FileNotFoundError:
        pass

def toggle():
    if is_enabled():
        disable()
        print("Startup disabled.")
    else:
        enable()
        print("Startup enabled.")


if __name__ == "__main__":
    print("Currently enabled:", is_enabled())
    toggle()
    print("Now enabled:", is_enabled())
