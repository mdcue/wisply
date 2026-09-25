import json, uuid
from datetime import datetime
from pathlib import Path

ACCOUNT_PATH = Path(__file__).parent.parent / "account.json"

def load_or_create_account():
    if ACCOUNT_PATH.exists():
        with open(ACCOUNT_PATH, "r") as f:
            return json.load(f)
    
    name = input("Welcome! What should I call you? ")
    account = {
        "id" : str(uuid.uuid4()),
        "name" : name,
        "created_at" : datetime.now().isoformat()
    }
    with open(ACCOUNT_PATH, "w") as f:
        json.dump(account, f, indent = 2)
    return account

if __name__ == "__main__":
    acc = load_or_create_account()
    print("Loaded account: ", acc)