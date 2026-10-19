import sys
import time
from datetime import datetime, timedelta, timezone

import requests

from core import db
from bots.common import USER_AGENT, register_wisp

HEADERS = {
    "User-Agent": USER_AGENT,
    "Accept": "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
}
BOT_ID = "github"
BOT_NAME = "Gitty"
BOT_COLOR = "#8957e5"
DESCRIPTION = "Tracks trending, rising and hidden-gems repositories on GitHub."

SEARCH_URL = "https://api.github.com/search/repositories"
REQUEST_GAP_SECONDS = 2
TOP_N = 5
DESCRIPTION_LIMIT = 50 #characters for each repo's description to show; 0 hides it
RISING_DAYS = 30
GEM_MIN_STARS = 50
GEM_MAX_STARS = 500
LANGUAGES = ["python"] #placeholder until the Hub has a language picker (preferably generalized and not language specific)

#(channel id, title, description, colour)
CHANNELS = [
    ("gh_day", "Trending today", "New repos gaining stars today", "#8957e5"),
    ("gh_week", "Trending this week", "New repos gaining stars this week", "#2196F3"),
    ("gh_month", "Trending this month", "New repos gaining stars this month", "#14b8a6"),
    ("gh_rising", "Rising", "Fastest-growing new repos by stars per day", "#f59e0b"),
    ("gh_gems", "Hidden gems", "Promising new repos that few people have found", "#ec4899"),
    ("gh_langs", "My languages", "New repos in your languages this week", "#4CAF50"),
]

def register():
    register_wisp(BOT_ID, BOT_NAME, BOT_COLOR, CHANNELS)

def _since(days):
    return (datetime.now(timezone.utc) - timedelta(days = days)).date().isoformat()

def _search(query, per_page):
    response = requests.get(
        SEARCH_URL,
        params = {"q": query, "sort": "stars", "order": "desc", "per_page": per_page},
        headers = HEADERS,
        timeout = 15,
    )
    if response.status_code in (403, 429):
        reset = response.headers.get("x-ratelimit-reset", "unknown")
        raise RuntimeError(f"GitHub refused the request (status {response.status_code}, limit resets at epoch {reset})")
    response.raise_for_status()
    return response.json().get("items", [])

def _shorten(text, limit):
    text = (text or "").strip()
    return text if len(text) <= limit else text[:limit - 1].rstrip() + "..."

def _compact(number):
    """845 -> 845, 4816 -> 4.8K, 45979 -> 46K, 1250000 -> 1.2M"""
    if number >= 1_000_000:
        return f"{number / 1_000_000:.1f}M".replace(".0M", "M")
    if number >= 10_000:
        return f"{number / 1000:.0f}k"
    if number >= 1000:
        return f"{number / 1000:.1f}k".replace(".0k", "k")
    return str(number)

def _stars_per_day(repo):
    created = datetime.fromisoformat(repo["created_at"].replace("Z", "+00:00"))
    age_days = max(1.0, (datetime.now(timezone.utc) - created).total_seconds() / 86400)
    return repo["stargazers_count"] / age_days

def _describe(repo, extra = ""):
    facts = [f"★ {_compact(repo['stargazers_count'])}{extra}"]
    if repo.get("language"):
        facts.append(repo["language"])
    text = f"{repo['full_name']}  ({', '.join(facts)})"
    description = _shorten(repo.get("description"), DESCRIPTION_LIMIT) if DESCRIPTION_LIMIT else ""
    return f"{text} - {description}" if description else text

def _save(channel_id, repos, extra = None):
    for repo in reversed(repos): #the Hub lists the newest first, so the best repo goes in last
        key = f"{channel_id}:{repo['id']}"
        if db.has_item(key):
            continue
        suffix = extra(repo) if extra else ""
        db.add_item(channel_id, _describe(repo, suffix), key, url = repo["html_url"], price = None, discount = repo["stargazers_count"])

def reset_items():
    """Deletes every item this wisp has stored, so the next fetch rebuilds them in the
    current format. Its keys all start with gh_ ('!' is just the escape characters, so the
    underscore is matched literally)."""
    conn = db.get_connection()
    conn.execute("DELETE FROM items WHERE timestamp LIKE 'gh!_%' ESCAPE '!'")
    conn.commit()
    conn.close()

def fetch():
    #One request feeds bot "last month" and "rising"
    month = _search(f"created:>={_since(RISING_DAYS)}", 100)
    _save("gh_month", month[:TOP_N])
    rising = sorted(month, key = _stars_per_day, reverse = True)[:TOP_N]
    _save("gh_rising", rising, lambda repo: f", ~{_compact(round(_stars_per_day(repo)))}/day")
    
    time.sleep(REQUEST_GAP_SECONDS)
    _save("gh_week", _search(f"created:>={_since(7)}", TOP_N))
    
    time.sleep(REQUEST_GAP_SECONDS)
    _save("gh_day", _search(f"created:>={_since(1)}", TOP_N))
    
    time.sleep(REQUEST_GAP_SECONDS)
    gems = _search(
        f"created:>={_since(RISING_DAYS)} stars:{GEM_MIN_STARS}..{GEM_MAX_STARS} archived:false", 30
    )
    _save("gh_gems", [repo for repo in gems if repo.get("description")][:TOP_N])
    
    found = {}
    for language in LANGUAGES:
        time.sleep(REQUEST_GAP_SECONDS)
        for repo in _search(f'created:>={_since(7)} language:"{language}"', TOP_N):
            found[repo["id"]] = repo
    top = sorted(found.values(), key = lambda repo: repo["stargazers_count"], reverse = True)[:TOP_N]
    _save("gh_langs", top)

if __name__ == "__main__":
    db.init_db()
    register()
    if "--reset" in sys.argv:
        reset_items()
        print("Cleared the old GitHub items.")
    fetch()
    for channel_id, title, _desc, _color in CHANNELS:
        print(f"Unread in {title}:", db.get_unread_count(channel_id))
