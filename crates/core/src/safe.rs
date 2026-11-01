//! Cleaning up anything that came from outside before it is stored or shown. A reply from a website
//! is not trusted: its text could hide line breaks or text-direction tricks, and its links could
//! point somewhere other than the site we asked.

use url::Url;

/// Longest item text that is kept.
pub const MAX_ITEM_TEXT: usize = 300;

/// Characters that change which way text runs (they can make a harmless-looking line read
/// differently from what it really says).
fn is_direction_control(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

/// One tidy line: control characters and text-direction tricks removed, runs of spaces and line
/// breaks turned into a single space, and cut to `max_chars` (ending in "…" if it was cut).
pub fn clean_text(text: &str, max_chars: usize) -> String {
    let spaced: String =
        text.chars().filter(|c| !is_direction_control(*c)).map(|c| if c.is_control() { ' ' } else { c }).collect();
    let line = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max_chars {
        return line;
    }
    let mut cut: String = line.chars().take(max_chars.saturating_sub(1)).collect();
    cut = cut.trim_end().to_string();
    cut.push('…');
    cut
}

/// The link as clean text if, and only if, it is an `https` link to one of `hosts` with no
/// username or password in it. Anything else gives `None`, and the item is stored without a link.
pub fn link_on_hosts(raw: &str, hosts: &[&str]) -> Option<String> {
    let url = Url::parse(raw.trim()).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    if hosts.iter().any(|allowed| allowed.eq_ignore_ascii_case(&host)) {
        Some(url.to_string())
    } else {
        None
    }
}