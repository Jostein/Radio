use crate::station::is_playable;
use std::time::Duration;
use url::Url;

const USER_AGENT: &str = "Radio/1.0";

/// Many stations hand out `.m3u` or `.pls` playlists instead of a stream URL.
/// Those documents are small text files; fetch one and take the first playable
/// stream address.
pub fn is_playlist(url: &Url) -> bool {
    match url.path().rsplit_once('.') {
        Some((_, extension)) => matches!(extension.to_ascii_lowercase().as_str(), "m3u" | "pls"),
        None => false,
    }
}

pub fn resolve(url: &Url) -> Result<Url, String> {
    let response = ureq::get(url.as_str())
        .set("User-Agent", USER_AGENT)
        .timeout(Duration::from_secs(10))
        .call()
        .map_err(|error| format!("The playlist could not be fetched ({error})."))?;

    let mut body = Vec::new();
    use std::io::Read;
    response
        .into_reader()
        .read_to_end(&mut body)
        .map_err(|error| format!("The playlist could not be read ({error})."))?;

    let text = String::from_utf8_lossy(&body);
    stream_in(&text, url).ok_or_else(|| "The playlist did not contain a stream URL.".to_string())
}

fn stream_in(text: &str, base: &Url) -> Option<Url> {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();

    // An HLS playlist served from an .m3u path: play the playlist itself.
    if lines.iter().any(|line| line.starts_with("#EXT-X-")) {
        return Some(base.clone());
    }

    for line in lines {
        let Some(candidate) = candidate(line) else {
            continue;
        };
        if let Ok(url) = base.join(candidate) {
            if is_playable(&url) {
                return Some(url);
            }
        }
    }
    None
}

/// The stream address on a playlist line, or `None` for comments, section
/// headers and non-stream metadata.
fn candidate(line: &str) -> Option<&str> {
    if line.starts_with('#') || line.starts_with('[') {
        return None;
    }

    let lowered = line.to_ascii_lowercase();
    if lowered.starts_with("http://") || lowered.starts_with("https://") {
        return Some(line);
    }

    if let Some(equals) = line.find('=') {
        let (key, value) = line.split_at(equals);
        // PLS entries look like `File1=http://…`; other `Key=Value` lines go.
        if !key.to_ascii_lowercase().starts_with("file") {
            return None;
        }
        return Some(value[1..].trim());
    }

    // A relative reference, resolved against the playlist URL.
    Some(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/list.pls").unwrap()
    }

    #[test]
    fn reads_pls_entries() {
        let text = "[playlist]\nNumberOfEntries=1\nFile1=https://example.com/stream.mp3\n";
        assert_eq!(
            stream_in(text, &base()).unwrap().as_str(),
            "https://example.com/stream.mp3"
        );
    }

    #[test]
    fn resolves_relative_references() {
        let text = "# comment\n/stream.mp3\n";
        assert_eq!(
            stream_in(text, &base()).unwrap().as_str(),
            "https://example.com/stream.mp3"
        );
    }

    #[test]
    fn keeps_hls_playlists() {
        let text = "#EXTM3U\n#EXT-X-VERSION:3\n";
        assert_eq!(stream_in(text, &base()).unwrap(), base());
    }

    #[test]
    fn ignores_metadata() {
        let text = "Title=Something\nLength=1\n";
        assert!(stream_in(text, &base()).is_none());
    }
}
