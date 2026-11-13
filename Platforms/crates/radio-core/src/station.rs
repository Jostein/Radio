use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

/// A station the user has saved.
///
/// The JSON keys match the macOS app (`id`, `title`, `urlString`), so an
/// existing `stations.json` can be copied between platforms unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub id: String,
    pub title: String,
    #[serde(rename = "urlString")]
    pub url_string: String,
}

impl Station {
    pub fn new(title: impl Into<String>, url_string: impl Into<String>) -> Self {
        Station {
            id: new_id(),
            title: title.into(),
            url_string: url_string.into(),
        }
    }

    /// The playable stream URL, or `None` when the saved string is not http(s).
    pub fn stream_url(&self) -> Option<Url> {
        stream_url(&self.url_string)
    }
}

pub fn stream_url(text: &str) -> Option<Url> {
    // Pasting carries stray whitespace and line breaks; URLs cannot contain them.
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let url = Url::parse(&cleaned).ok()?;
    if is_playable(&url) {
        Some(url)
    } else {
        None
    }
}

pub fn is_playable(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
}

/// A short unique id without pulling in a uuid dependency.
fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}", nanos, seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_whitespace_and_requires_http() {
        assert!(stream_url(" https://example.com/stream \n").is_some());
        assert!(stream_url("file:///tmp/song.mp3").is_none());
        assert!(stream_url("example.com/stream").is_none());
    }
}
