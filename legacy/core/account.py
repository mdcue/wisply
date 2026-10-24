import json, uuid
from datetime import datetime

from core.paths import DATA_DIR

ACCOUNT_PATH = DATA_DIR / "account.json"

def account_exists():
    return ACCOUNT_PATH.exists()

def get_name():
    """The name give at setup, or None if there is no account (or it can't be read)."""
    try:
        with open(ACCOUNT_PATH, encoding = "utf-8") as f:
            return json.load(f).get("name") or None
    except (OSError, ValueError):
        return None

def create_account(name):
    account = {
        "id": str(uuid.uuid4()),
        "name": name,
        "created_at": datetime.now().isoformat()
    }
    with open(ACCOUNT_PATH, "w") as f:
        json.dump(account, f, indent = 2)
    return account