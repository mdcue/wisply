//! GitHub ("Gitty"): new repositories that are trending, rising or still undiscovered, found with
//! GitHub's search. One search is made per channel (the month search also feeds "Rising"), with a
//! short pause between them so GitHub isn't hit in a burst.

use std::thread;
use std::time::Duration;

use chrono::{DateTime, Duration as Days, Utc};
use rusqlite::Connection;
use serde_json::Value;
use url::Url;

use super::{register, ChannelDef, WispDef};
use crate::http::Client;
use crate::safe::{clean_text, link_on_hosts};
use crate::scheduler::Fetcher;
use crate::store::{self, NewItem};

pub const DEF: WispDef = WispDef {
    id: "github",
    name: "Gitty",
    color: "#8957e5",
    description: "Tracks trending, rising and hidden-gems repositories on GitHub.",
    channels: &[
        ChannelDef { id: "gh_day", title: "Trending today", description: "New repos gaining stars today", color: "#8957e5" },
        ChannelDef { id: "gh_week", title: "Trending this week", description: "New repos gaining stars this week", color: "#2196F3" },
        ChannelDef { id: "gh_month", title: "Trending this month", description: "New repos gaining stars this month", color: "#14b8a6" },
        ChannelDef { id: "gh_rising", title: "Rising", description: "Fastest-growing new repos by stars per day", color: "#f59e0b" },
        ChannelDef { id: "gh_gems", title: "Hidden gems", description: "Promising new repos that few people have found", color: "#ec4899" },
        ChannelDef { id: "gh_langs", title: "My languages", description: "New repos in your languages this week", color: "#4CAF50" },
    ],
    expression: "smiling",
    fetch_interval: None,
};

const SEARCH_URL: &str = "https://api.github.com/search/repositories";
const HOST: &str = "api.github.com";
/// Links are only kept when they point here.
const LINK_HOST: &str = "github.com";
const REQUEST_GAP: Duration = Duration::from_secs(2);
/// How many repos each channel keeps.
const TOP_N: usize = 5;
/// Characters of a repo's description to show; 0 hides it.
const DESCRIPTION_LIMIT: usize = 50;
const RISING_DAYS: i64 = 30;
const GEM_MIN_STARS: i64 = 50;
const GEM_MAX_STARS: i64 = 500;
/// A placeholder until the Hub has a language picker.
const LANGUAGES: [&str; 1] = ["python"];
/// Repos asked for in the searches that keep only `TOP_N`, so that after junk is left out there
/// are still enough good ones to fill the channel.
const SEARCH_POOL: usize = 10;
/// A repo gaining stars faster than this (stars a day, a new repo counting as one day old at
/// least) while nobody has forked it looks like bought stars, not a real crowd.
const STAR_FARM_PER_DAY: f64 = 500.0;
/// Wording that marks a repo as bait on its own: pirated software, game cheats and "free money"
/// generators. Compared with whole words only, in lower case, so "cheat sheet" is not caught.
const JUNK_PHRASES: &[&str] = &[
    "free robux",
    "robux generator",
    "free vbucks",
    "vbucks generator",
    "hack tool",
    "hacktool",
    "free generator",
    "free account generator",
    "money generator",
    "game hack",
    "game cheat",
    "cheat menu",
    "cheat loader",
    "aimbot",
    "wallhack",
    "warez",
    "nulled",
    "free airdrop",
    "airdrop claim",
    "airdrop bot",
];
/// Words that only count as junk together with a giveaway word in the same place ("Photoshop
/// Crack Free Download"), because alone they are often honest ("crack detection", "ssh-keygen").
const CRACK_WORDS: &[&str] = &["crack", "cracked", "keygen"];
const GIVEAWAY_WORDS: &[&str] = &["free", "download", "full", "activation", "serial", "license", "torrent", "patch"];

pub fn register_wisp(conn: &Connection) -> crate::Result<()> {
    register(conn, &DEF)
}

/// Deletes every item this wisp has stored, so the next check rebuilds them in the current
/// format. Returns how many were removed. (Its keys all start with `gh_`; the `!` is only the
/// escape character, so the underscore is matched literally and not as "any character".)
pub fn reset_items(conn: &Connection) -> crate::Result<usize> {
    Ok(conn.execute("DELETE FROM items WHERE dedupe_key LIKE 'gh!_%' ESCAPE '!'", [])?)
}

/// The parts of a repository we use. A repo missing one of the essentials is left out.
#[derive(Debug, Clone)]
struct Repo {
    id: String,
    full_name: String,
    stars: i64,
    html_url: String,
    created_at: Option<DateTime<Utc>>,
    language: Option<String>,
    description: Option<String>,
    /// How many times it was forked, when GitHub says. `None` means unknown, never "zero".
    forks: Option<i64>,
    fork: bool,
    archived: bool,
    disabled: bool,
    topics: Vec<String>,
}

fn parse_repo(value: &Value) -> Option<Repo> {
    let text = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_string);
    let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    Some(Repo {
        id: value.get("id")?.as_i64()?.to_string(),
        full_name: text("full_name").filter(|n| !n.trim().is_empty())?,
        stars: value.get("stargazers_count")?.as_i64()?.max(0),
        html_url: text("html_url")?,
        created_at: text("created_at")
            .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
            .map(|t| t.with_timezone(&Utc)),
        language: text("language").filter(|l| !l.trim().is_empty()),
        description: text("description"),
        forks: value.get("forks_count").and_then(Value::as_i64),
        fork: flag("fork"),
        archived: flag("archived"),
        disabled: flag("disabled"),
        topics: value
            .get("topics")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default(),
    })
}

/// Why a repo was left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Junk {
    Fork,
    Archived,
    Disabled,
    /// Pirate, cheat or "free money" wording in its name, description or topics.
    BaitWords,
    /// Many stars very fast and nobody has forked it.
    StarFarm,
}

impl Junk {
    /// A few plain words for the log.
    pub fn label(self) -> &'static str {
        match self {
            Junk::Fork => "a fork",
            Junk::Archived => "archived",
            Junk::Disabled => "disabled",
            Junk::BaitWords => "bait wording",
            Junk::StarFarm => "many stars but no forks",
        }
    }
}

/// A repo that was left out during a check, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub name: String,
    pub why: Junk,
}

/// Lower-cases `text` and cuts it into whole words (letters and digits), so "Crack-Tool_v2"
/// becomes `crack`, `tool`, `v2`.
fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

/// Is the phrase (words separated by single spaces) in the list of words, whole words in a row?
fn contains_phrase(words: &[String], phrase: &str) -> bool {
    let wanted: Vec<&str> = phrase.split(' ').collect();
    !wanted.is_empty() && words.windows(wanted.len()).any(|run| run.iter().zip(&wanted).all(|(a, b)| a == b))
}

/// Does this one piece of text (a name, a description, all the topics) read like bait?
fn text_is_bait(text: &str) -> bool {
    let words = words_of(text);
    JUNK_PHRASES.iter().any(|phrase| contains_phrase(&words, phrase))
        || (words.iter().any(|w| CRACK_WORDS.contains(&w.as_str()))
            && words.iter().any(|w| GIVEAWAY_WORDS.contains(&w.as_str())))
}

fn has_bait_words(repo: &Repo) -> bool {
    text_is_bait(&repo.full_name)
        || repo.description.as_deref().is_some_and(text_is_bait)
        || (!repo.topics.is_empty() && text_is_bait(&repo.topics.join(" ")))
}

/// The reason to leave a repo out, or `None` to keep it. Only fields GitHub already sends with
/// the search are used, so this costs no extra requests.
fn junk_reason(repo: &Repo, now: DateTime<Utc>) -> Option<Junk> {
    if repo.fork {
        Some(Junk::Fork)
    } else if repo.archived {
        Some(Junk::Archived)
    } else if repo.disabled {
        Some(Junk::Disabled)
    } else if has_bait_words(repo) {
        Some(Junk::BaitWords)
    } else if repo.forks == Some(0) && stars_per_day(repo, now) >= STAR_FARM_PER_DAY {
        Some(Junk::StarFarm)
    } else {
        None
    }
}

pub struct GithubFetcher {
    client: Client,
    search_url: String,
    gap: Duration,
    /// `None` means "the real time"; tests set a fixed one.
    now: Option<DateTime<Utc>>,
    languages: Vec<String>,
    /// The repos left out during the latest check (each once), for the log and for tests.
    skipped: Vec<Skipped>,
}

impl GithubFetcher {
    pub fn new() -> GithubFetcher {
        GithubFetcher {
            client: Client::strict(&[HOST]),
            search_url: SEARCH_URL.to_string(),
            gap: REQUEST_GAP,
            now: None,
            languages: LANGUAGES.iter().map(|l| l.to_string()).collect(),
            skipped: Vec::new(),
        }
    }

    /// For tests: a client, a search address of your own (a fake website on this computer), and
    /// the pause between searches (zero, so tests don't wait).
    pub fn with_client(client: Client, search_url: &str, gap: Duration) -> GithubFetcher {
        GithubFetcher { client, search_url: search_url.to_string(), gap, ..GithubFetcher::new() }
    }

    /// For tests: pretend it is this moment, so the date ranges asked for are always the same.
    pub fn with_now(mut self, now: DateTime<Utc>) -> GithubFetcher {
        self.now = Some(now);
        self
    }

    pub fn with_languages(mut self, languages: &[&str]) -> GithubFetcher {
        self.languages = languages.iter().map(|l| l.to_string()).collect();
        self
    }

    fn now(&self) -> DateTime<Utc> {
        self.now.unwrap_or_else(Utc::now)
    }

    /// The repos left out during the latest check, each once, with the reason.
    pub fn skipped(&self) -> &[Skipped] {
        &self.skipped
    }

    /// Drops the junk from one search's repos and remembers each one dropped (once per check,
    /// even if several searches found it).
    fn screen(&mut self, repos: Vec<Repo>) -> Vec<Repo> {
        let now = self.now();
        let mut kept = Vec::with_capacity(repos.len());
        for repo in repos {
            match junk_reason(&repo, now) {
                None => kept.push(repo),
                Some(why) => {
                    if !self.skipped.iter().any(|s| s.name == repo.full_name) {
                        self.skipped.push(Skipped { name: repo.full_name.clone(), why });
                    }
                }
            }
        }
        kept
    }

    /// The date `days` days ago, as GitHub's search wants it (`2026-10-25`).
    fn since(&self, days: i64) -> String {
        (self.now() - Days::days(days)).date_naive().format("%Y-%m-%d").to_string()
    }

    fn pause(&self) {
        if !self.gap.is_zero() {
            thread::sleep(self.gap);
        }
    }

    /// One search, best-starred first. Returns the repos it found.
    fn search(&self, query: &str, per_page: usize) -> Result<Vec<Repo>, String> {
        let per_page = per_page.to_string();
        let url = Url::parse_with_params(
            &self.search_url,
            [("q", query), ("sort", "stars"), ("order", "desc"), ("per_page", per_page.as_str())],
        )
        .map_err(|e| format!("bad address: {e}"))?;
        let reply = self
            .client
            .get(&url, &[("Accept", "application/vnd.github+json"), ("X-GitHub-Api-Version", "2022-11-28")])?;
        if reply.status == 403 || reply.status == 429 {
            let reset = reply.header("x-ratelimit-reset").unwrap_or("unknown");
            return Err(format!(
                "GitHub refused the request (status {}, limit resets at epoch {})",
                reply.status,
                clean_text(reset, 20)
            ));
        }
        if !reply.is_success() {
            return Err(format!("GitHub answered with status {}", reply.status));
        }
        let json: Value =
            serde_json::from_str(&reply.body).map_err(|_| "GitHub sent something that isn't JSON".to_string())?;
        match json.get("items") {
            None | Some(Value::Null) => Ok(Vec::new()),
            Some(Value::Array(list)) => Ok(list.iter().filter_map(parse_repo).collect()),
            Some(_) => Err("GitHub's list of repositories was not a list".to_string()),
        }
    }
}

impl Default for GithubFetcher {
    fn default() -> Self {
        GithubFetcher::new()
    }
}

impl Fetcher for GithubFetcher {
    fn wisp_id(&self) -> &str {
        DEF.id
    }

    fn fetch(&mut self, conn: &Connection) -> Result<(), String> {
        let now = self.now();
        self.skipped.clear();

        // One search feeds both "this month" and "rising".
        let month = self.search(&format!("created:>={}", self.since(RISING_DAYS)), 100)?;
        let month = self.screen(month);
        save(conn, "gh_month", &month[..month.len().min(TOP_N)], None)?;
        let mut rising: Vec<&Repo> = month.iter().collect();
        rising.sort_by(|a, b| stars_per_day(b, now).total_cmp(&stars_per_day(a, now)));
        let rising: Vec<Repo> = rising.into_iter().take(TOP_N).cloned().collect();
        let per_day = |repo: &Repo| format!(", ~{}/day", compact(stars_per_day(repo, now).round_ties_even() as i64));
        save(conn, "gh_rising", &rising, Some(&per_day))?;

        self.pause();
        let week = self.search(&format!("created:>={}", self.since(7)), SEARCH_POOL)?;
        let week = self.screen(week);
        save(conn, "gh_week", &week[..week.len().min(TOP_N)], None)?;

        self.pause();
        let day = self.search(&format!("created:>={}", self.since(1)), SEARCH_POOL)?;
        let day = self.screen(day);
        save(conn, "gh_day", &day[..day.len().min(TOP_N)], None)?;

        self.pause();
        let gems = self.search(
            &format!(
                "created:>={} stars:{GEM_MIN_STARS}..{GEM_MAX_STARS} archived:false",
                self.since(RISING_DAYS)
            ),
            30,
        )?;
        let gems = self.screen(gems);
        let gems: Vec<Repo> = gems
            .into_iter()
            .filter(|r| r.description.as_deref().is_some_and(|d| !d.is_empty()))
            .take(TOP_N)
            .collect();
        save(conn, "gh_gems", &gems, None)?;

        let mut found: Vec<Repo> = Vec::new();
        for language in self.languages.clone() {
            self.pause();
            let query = format!("created:>={} language:\"{}\"", self.since(7), language.replace('"', ""));
            let in_language = self.search(&query, SEARCH_POOL)?;
            for repo in self.screen(in_language) {
                // The same repo found twice counts once (the later copy replaces the earlier).
                match found.iter().position(|f| f.id == repo.id) {
                    Some(at) => found[at] = repo,
                    None => found.push(repo),
                }
            }
        }
        found.sort_by_key(|repo| std::cmp::Reverse(repo.stars));
        found.truncate(TOP_N);
        save(conn, "gh_langs", &found, None)?;
        Ok(())
    }
}

/// A star count in a short form: 845, 4.8k, 46k, 1.2M.
pub fn compact(number: i64) -> String {
    // Counted in tenths and rounded half-to-even (the way Python's formatting did), so that
    // 1250 shows as 1.2k.
    fn tenths(tenths: i64, suffix: &str) -> String {
        if tenths % 10 == 0 {
            format!("{}{suffix}", tenths / 10)
        } else {
            format!("{}.{}{suffix}", tenths / 10, tenths % 10)
        }
    }
    let n = number.max(0);
    if n >= 1_000_000 {
        tenths((n as f64 / 100_000.0).round_ties_even() as i64, "M")
    } else if n >= 10_000 {
        let thousands = (n as f64 / 1000.0).round_ties_even() as i64;
        // 999,500 and up rounds to 1000k, which reads better as 1M.
        if thousands >= 1000 {
            "1M".to_string()
        } else {
            format!("{thousands}k")
        }
    } else if n >= 1000 {
        tenths((n as f64 / 100.0).round_ties_even() as i64, "k")
    } else {
        n.to_string()
    }
}

/// How fast a repo has gained stars: stars divided by its age in days (a new repo counts as one
/// day old at least). A repo with no readable creation date counts as 0.
fn stars_per_day(repo: &Repo, now: DateTime<Utc>) -> f64 {
    let Some(created) = repo.created_at else { return 0.0 };
    let age_days = ((now - created).num_seconds() as f64 / 86_400.0).max(1.0);
    repo.stars as f64 / age_days
}

/// Cuts a description to `limit` characters, ending in "..." if it was cut.
fn shorten(text: &str, limit: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let cut: String = text.chars().take(limit.saturating_sub(1)).collect();
    format!("{}...", cut.trim_end())
}

/// The line shown for a repo: `owner/name  (★ 4.8k, Rust) - what it does`.
fn describe(repo: &Repo, extra: &str) -> String {
    let mut facts = vec![format!("★ {}{}", compact(repo.stars), extra)];
    if let Some(language) = &repo.language {
        facts.push(clean_text(language, 40));
    }
    let text = format!("{}  ({})", clean_text(&repo.full_name, 120), facts.join(", "));
    let description = match (&repo.description, DESCRIPTION_LIMIT) {
        (Some(d), limit) if limit > 0 => shorten(&clean_text(d, 400), limit),
        _ => String::new(),
    };
    if description.is_empty() {
        text
    } else {
        format!("{text} - {description}")
    }
}

/// Stores the repos in a channel. The best repo goes in last, so the Hub (newest first) lists it
/// first. A repo already stored (or dismissed) is left alone.
fn save(
    conn: &Connection,
    channel_id: &str,
    repos: &[Repo],
    extra: Option<&dyn Fn(&Repo) -> String>,
) -> Result<usize, String> {
    let mut added = 0;
    for repo in repos.iter().rev() {
        let suffix = extra.map(|f| f(repo)).unwrap_or_default();
        let item = NewItem {
            channel_id: channel_id.to_string(),
            dedupe_key: format!("{channel_id}:{}", repo.id),
            text: describe(repo, &suffix),
            url: link_on_hosts(&repo.html_url, &[LINK_HOST]),
            stars: Some(repo.stars),
            ..Default::default()
        };
        if store::add_item(conn, &item).map_err(|e| e.to_string())? {
            added += 1;
        }
    }
    Ok(added)
}