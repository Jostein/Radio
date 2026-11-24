//! Station lookup by name against the public Radio Browser directory.
//!
//! The macOS app asks this directory when you want a station you have not saved
//! yet: one query per search, nothing fetched at launch and nothing cached after
//! the results close. This is that query, with the same parameters, the same
//! limits and the same rule about what counts as a usable answer.

use serde::Deserialize;
use std::io::Read;
use std::time::Duration;
use url::Url;

const ENDPOINT: &str = "https://de1.api.radio-browser.info/json/stations/search";
const USER_AGENT: &str = "Radio/1.0";
const TIMEOUT: Duration = Duration::from_secs(10);
/// Thirty stations of the directory's own JSON is well under this; the cap is
/// here so an unexpected answer cannot be downloaded in full.
const MAXIMUM_BYTES: usize = 512 * 1024;

/// One station as the directory describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub name: String,
    pub country: String,
    pub codec: String,
    pub bitrate: u32,
    pub url: String,
}

impl Found {
    /// The second line a result shows: what it plays, how well, and where from.
    pub fn detail(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.codec.is_empty() {
            parts.push(self.codec.clone());
        }
        if self.bitrate > 0 {
            parts.push(format!("{} kbps", self.bitrate));
        }
        parts.push(self.url.clone());
        parts.join(" · ")
    }

    /// The name plus the country, when the directory knows one.
    pub fn label(&self) -> String {
        if self.country.is_empty() {
            self.name.clone()
        } else {
            format!("{} — {}", self.name, self.country)
        }
    }
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    name: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    codec: String,
    #[serde(default)]
    bitrate: u32,
    #[serde(default)]
    url_resolved: String,
    #[serde(default)]
    url: String,
}

/// The query for a name, or `None` when there is nothing to look for.
///
/// `hidebroken` keeps servers the directory has failed to reach out of the
/// answer, and `clickcount` puts the stations other people actually listen to
/// first, which is the ordering the macOS app uses.
pub fn query_url(query: &str, limit: usize) -> Option<Url> {
    let text = query.trim();
    if text.is_empty() {
        return None;
    }

    let mut url = Url::parse(ENDPOINT).ok()?;
    url.query_pairs_mut()
        .append_pair("name", text)
        .append_pair("limit", &limit.to_string())
        .append_pair("hidebroken", "true")
        .append_pair("order", "clickcount")
        .append_pair("reverse", "true");
    Some(url)
}

/// Reads the directory's answer, keeping only entries that can be played and
/// only one copy of each stream.
pub fn parse(body: &[u8]) -> Vec<Found> {
    let Ok(entries) = serde_json::from_slice::<Vec<Raw>>(body) else {
        return Vec::new();
    };

    let mut seen: Vec<String> = Vec::new();
    let mut found = Vec::new();

    for entry in entries {
        let name = entry.name.trim().to_string();
        if name.is_empty() {
            continue;
        }

        let address = if entry.url_resolved.trim().is_empty() {
            entry.url.trim().to_string()
        } else {
            entry.url_resolved.trim().to_string()
        };
        if crate::station::stream_url(&address).is_none() {
            continue;
        }
        if seen.iter().any(|url| url == &address) {
            continue;
        }
        seen.push(address.clone());

        found.push(Found {
            name,
            country: entry.country.trim().to_string(),
            codec: entry.codec.trim().to_string(),
            bitrate: entry.bitrate,
            url: address,
        });
    }

    found
}

/// Runs the query. A directory that cannot be reached is reported, not guessed
/// at: an empty list and a failure mean different things to the caller.
pub fn search(query: &str, limit: usize) -> Result<Vec<Found>, String> {
    let Some(url) = query_url(query, limit) else {
        return Ok(Vec::new());
    };

    let response = ureq::get(url.as_str())
        .set("User-Agent", USER_AGENT)
        .timeout(TIMEOUT)
        .call()
        .map_err(|error| format!("The station directory could not be reached: {error}"))?;

    let mut reader = response.into_reader();
    let mut body = Vec::new();
    let mut chunk = [0u8; 8192];

    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                if body.len() + count > MAXIMUM_BYTES {
                    return Err("The directory answer was too large.".to_string());
                }
                body.extend_from_slice(&chunk[..count]);
            }
            Err(error) => return Err(format!("The directory answer stopped early: {error}")),
        }
    }

    Ok(parse(&body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_query_with_the_same_parameters_as_the_mac() {
        let url = query_url("  radio swiss jazz  ", 30).unwrap();
        let text = url.as_str();
        assert!(text.starts_with(ENDPOINT));
        assert!(text.contains("name=radio+swiss+jazz"));
        assert!(text.contains("limit=30"));
        assert!(text.contains("hidebroken=true"));
        assert!(text.contains("order=clickcount"));
        assert!(text.contains("reverse=true"));
    }

    #[test]
    fn ignores_a_blank_query() {
        assert_eq!(query_url("   ", 30), None);
    }

    #[test]
    fn keeps_playable_entries_once() {
        let body = br#"[
            {"name":"One","country":"NO","codec":"MP3","bitrate":128,
             "url":"https://example.com/a","url_resolved":"https://example.com/a"},
            {"name":"One again","country":"NO","codec":"MP3","bitrate":128,
             "url":"https://example.com/a","url_resolved":"https://example.com/a"},
            {"name":"","country":"","codec":"","bitrate":0,
             "url":"https://example.com/b","url_resolved":""},
            {"name":"Broken","country":"","codec":"","bitrate":0,
             "url":"not a url","url_resolved":""},
            {"name":"Two","country":"DE","codec":"AAC","bitrate":0,
             "url":"https://example.com/c","url_resolved":""}
        ]"#;

        let found = parse(body);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "One");
        assert_eq!(found[0].bitrate, 128);
        assert_eq!(found[1].name, "Two");
        assert_eq!(found[1].url, "https://example.com/c");
    }

    #[test]
    fn describes_a_result_without_empty_parts() {
        let found = Found {
            name: "Nrk".to_string(),
            country: "NO".to_string(),
            codec: "MP3".to_string(),
            bitrate: 192,
            url: "https://example.com/nrk".to_string(),
        };
        assert_eq!(found.label(), "Nrk — NO");
        assert_eq!(found.detail(), "MP3 · 192 kbps · https://example.com/nrk");

        let sparse = Found {
            codec: String::new(),
            bitrate: 0,
            ..found
        };
        assert_eq!(sparse.detail(), "https://example.com/nrk");
    }
}
