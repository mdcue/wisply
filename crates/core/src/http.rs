//! The one place that talks to the internet. Every wisp fetches through this, so the safety rules
//! live here once instead of being repeated (and forgotten) in each wisp:
//!
//! * only the sites a wisp names are allowed, and only over `https`;
//! * redirects are never followed (a trusted site could otherwise bounce us somewhere else);
//! * a reply bigger than `MAX_BODY_BYTES` is refused, so a bad reply can't use up memory;
//! * every request has a 15 second limit and says who is asking (`USER_AGENT`).
//!
//! Errors come back as plain sentences, because that is what ends up in the log.

use std::time::Duration;

use chrono::DateTime;
use url::Url;

/// Sent with every request so the sites we call can tell who is asking. Replace the contact
/// before release; this is the only place it lives.
pub const USER_AGENT: &str = "wisply/0.1.0 (+https://github.com/mdcue/wisply)";
pub const MAX_BODY_BYTES: u64 = 2_000_000;
pub const TIMEOUT: Duration = Duration::from_secs(15);

/// What came back. The status is not turned into an error here, so a wisp can react to, say,
/// "rate limited" in its own way.
#[derive(Debug, Clone)]
pub struct Reply {
    pub status: u16,
    pub body: String,
    headers: Vec<(String, String)>,
}

impl Reply {
    /// A header's value (the name is matched ignoring upper/lower case).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// How many seconds the site asked us to wait before asking again, if it said so. `now` is the
    /// current time as unix seconds.
    ///
    /// * `Retry-After` comes first, either as a number of seconds or as a date;
    /// * otherwise, if `x-ratelimit-remaining` is 0, the wait runs until `x-ratelimit-reset`
    ///   (a unix time, as GitHub sends it);
    /// * otherwise the site said nothing, and the answer is `None`.
    ///
    /// A time already in the past gives 0. The number is only what the site said: whoever uses it
    /// decides the upper limit (see `scheduler::MAX_SERVER_WAIT_SECONDS`).
    pub fn wait_seconds(&self, now: i64) -> Option<i64> {
        if let Some(value) = self.header("retry-after") {
            let value = value.trim();
            if let Ok(seconds) = value.parse::<i64>() {
                return Some(seconds.max(0));
            }
            if let Ok(date) = DateTime::parse_from_rfc2822(value) {
                return Some((date.timestamp() - now).max(0));
            }
        }
        if self.header("x-ratelimit-remaining").map(str::trim) == Some("0") {
            if let Some(reset) = self.header("x-ratelimit-reset").and_then(|v| v.trim().parse::<i64>().ok()) {
                return Some((reset - now).max(0));
            }
        }
        None
    }
}

pub struct Client {
    agent: ureq::Agent,
    /// Empty means "any site" (only the test client does that).
    hosts: Vec<String>,
    https_only: bool,
}

fn agent(https_only: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .https_only(https_only)
        .user_agent(USER_AGENT)
        .build()
        .into()
}

impl Client {
    /// The client the app uses: `https` only, and only to the listed hosts.
    pub fn strict(hosts: &[&str]) -> Client {
        Client { agent: agent(true), hosts: hosts.iter().map(|h| h.to_ascii_lowercase()).collect(), https_only: true }
    }

    /// A client that may talk to any address over plain `http`. Only for tests, which run a small
    /// fake website on this computer. Never use it in the app.
    pub fn insecure_for_tests() -> Client {
        Client { agent: agent(false), hosts: Vec::new(), https_only: false }
    }

    /// Fetches `url`. `headers` are extra request headers, for example `("Accept", "...")`.
    pub fn get(&self, url: &Url, headers: &[(&str, &str)]) -> Result<Reply, String> {
        if self.https_only && url.scheme() != "https" {
            return Err(format!("refused to fetch {}: only https is allowed", url.host_str().unwrap_or("?")));
        }
        if !self.hosts.is_empty() {
            let host = url.host_str().unwrap_or("").to_ascii_lowercase();
            if !self.hosts.contains(&host) {
                return Err(format!("refused to fetch from {host}: it is not on the allowed list"));
            }
        }
        let mut request = self.agent.get(url.as_str());
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let mut response = request.call().map_err(|e| format!("request failed: {e}"))?;
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| Some((name.as_str().to_string(), value.to_str().ok()?.to_string())))
            .collect();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES)
            .read_to_string()
            .map_err(|e| format!("could not read the reply: {e}"))?;
        Ok(Reply { status, body, headers })
    }
}