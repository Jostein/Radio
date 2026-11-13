//! The best-guess station name for a URL, used to fill in the add dialog.
//!
//! Mirrors the macOS `StreamMetadata`: an Icecast/Shoutcast stream advertises
//! `icy-name` in its response headers, a `.m3u`/`.pls` playlist usually carries
//! a title of its own, and when a server answers but names nothing the URL path
//! becomes a readable fallback.

use crate::playlist;
use std::time::Duration;
use url::Url;

const USER_AGENT: &str = "Radio/1.0";

/// What a station calls itself, or `None` when nothing answered in time.
///
/// Blocking on purpose: the caller runs it on a worker thread and applies the
/// answer only if the URL has not changed since.
pub fn suggested_title(url: &Url) -> Option<String> {
    let (announced, reachable) = probe(url);
    match announced {
        Some(title) => Some(title),
        None if reachable => fallback_title(url),
        None => None,
    }
}

fn probe(url: &Url) -> (Option<String>, bool) {
    if playlist::is_playlist(url) {
        let Ok(response) = ureq::get(url.as_str())
            .set("User-Agent", USER_AGENT)
            .timeout(Duration::from_secs(10))
            .call()
        else {
            return (None, false);
        };

        let mut body = String::new();
        use std::io::Read;
        if response.into_reader().read_to_string(&mut body).is_err() {
            return (None, true);
        }
        return (playlist_title(&body), true);
    }

    match ureq::get(url.as_str())
        .set("User-Agent", USER_AGENT)
        .set("Icy-MetaData", "0")
        .timeout(Duration::from_secs(8))
        .call()
    {
        Ok(response) => (icy_name(&response), true),
        Err(_) => (None, false),
    }
}

fn icy_name(response: &ureq::Response) -> Option<String> {
    ["icy-name", "ice-name", "x-audiocast-name"]
        .iter()
        .find_map(|header| response.header(header))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// `#EXTINF:-1,Station Name` in an M3U, or `Title1=Station Name` in a PLS.
pub fn playlist_title(text: &str) -> Option<String> {
    for line in text.lines().map(str::trim) {
        let lowered = line.to_lowercase();

        if lowered.starts_with("#extinf") {
            if let Some((_, title)) = line.rsplit_once(',') {
                let title = title.trim();
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }

        if lowered.starts_with("title") {
            if let Some((_, title)) = line.split_once('=') {
                let title = title.trim();
                if !title.is_empty() {
                    return Some(title.to_string());
                }
            }
        }
    }
    None
}

/// The last readable path segment, minus its extension, in title case.
pub fn fallback_title(url: &Url) -> Option<String> {
    const GENERIC: [&str; 15] = [
        "stream", "streams", "live", "listen", "audio", "radio", "index", "playlist", "default",
        "mp3", "aac", "hls", "m3u8", "m3u", "pls",
    ];

    let mut parts: Vec<String> = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect();

    if let Some(last) = parts.pop() {
        let stem = last.rsplit_once('.').map_or(last.as_str(), |(stem, _)| stem);
        parts.push(stem.to_string());
    }

    for raw in parts.iter().rev() {
        let cleaned = raw.replace(['_', '-'], " ");
        let cleaned = cleaned.trim();
        let lowered = cleaned.to_lowercase();

        if cleaned.chars().count() < 3
            || !cleaned.chars().any(char::is_alphabetic)
            || GENERIC.contains(&lowered.as_str())
        {
            continue;
        }

        return Some(title_case(cleaned));
    }

    None
}

fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_title_from_an_m3u() {
        let text = "#EXTM3U\n#EXTINF:-1,FIP Reggae\nhttps://example.com/live.m3u8\n";
        assert_eq!(playlist_title(text), Some("FIP Reggae".to_string()));
    }

    #[test]
    fn reads_a_title_from_a_pls() {
        let text = "[playlist]\nNumberOfEntries=1\nFile1=https://example.com/a\nTitle1=NTS Radio 1\n";
        assert_eq!(playlist_title(text), Some("NTS Radio 1".to_string()));
    }

    #[test]
    fn ignores_playlists_without_a_title() {
        assert_eq!(playlist_title("#EXTM3U\nhttps://example.com/a"), None);
    }

    #[test]
    fn falls_back_to_a_readable_path_segment() {
        let url = Url::parse("https://example.com/streams/dance_radio/stream.mp3").unwrap();
        assert_eq!(fallback_title(&url), Some("Dance Radio".to_string()));
    }

    #[test]
    fn refuses_generic_paths() {
        let url = Url::parse("https://example.com/stream").unwrap();
        assert_eq!(fallback_title(&url), None);
    }
}
