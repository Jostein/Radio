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
    let text = fetch(url)?;
    stream_in(&text, url).ok_or_else(|| "The playlist did not contain a stream URL.".to_string())
}

fn fetch(url: &Url) -> Result<String, String> {
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

    Ok(String::from_utf8_lossy(&body).into_owned())
}

fn is_hls(url: &Url) -> bool {
    url.path().to_ascii_lowercase().ends_with(".m3u8")
}

/// The address to actually play. An HLS master playlist offers several
/// renditions and ffmpeg takes the first one, which is the lowest quality;
/// Apple players climb to the best, so pick the highest-bandwidth variant.
/// Anything else, or a failed fetch, plays as given.
pub fn preferred(url: &Url) -> Url {
    if !is_hls(url) {
        return url.clone();
    }
    fetch(url)
        .ok()
        .and_then(|text| variant_in(&text, url))
        .unwrap_or_else(|| url.clone())
}

/// The highest-bandwidth `#EXT-X-STREAM-INF` rendition, resolved against the
/// playlist. Media playlists list no renditions and answer `None`.
fn variant_in(text: &str, base: &Url) -> Option<Url> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let mut best: Option<(u64, Url)> = None;

    for (index, line) in lines.iter().enumerate() {
        let Some(attributes) = line.strip_prefix("#EXT-X-STREAM-INF:") else {
            continue;
        };
        let Some(bandwidth) = bandwidth_in(attributes) else {
            continue;
        };
        let Some(uri) = lines
            .get(index + 1)
            .filter(|next| !next.is_empty() && !next.starts_with('#'))
        else {
            continue;
        };
        let Ok(url) = base.join(uri) else {
            continue;
        };
        if best.as_ref().map_or(true, |(current, _)| bandwidth > *current) {
            best = Some((bandwidth, url));
        }
    }

    best.map(|(_, url)| url)
}

fn bandwidth_in(attributes: &str) -> Option<u64> {
    attributes
        .split(',')
        .find_map(|pair| pair.trim().strip_prefix("BANDWIDTH="))
        .and_then(|value| value.trim().parse().ok())
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
    fn picks_the_highest_hls_variant() {
        let text = "#EXTM3U\n\
            #EXT-X-STREAM-INF:PROGRAM-ID=0,BANDWIDTH=107000,CODECS=\"mp4a.40.2\"\n\
            low.m3u8\n\
            #EXT-X-STREAM-INF:PROGRAM-ID=0,BANDWIDTH=280000,CODECS=\"mp4a.40.2\"\n\
            https://cdn.example.com/high.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=185000\n\
            mid.m3u8\n";
        let base = Url::parse("https://example.com/radio/master.m3u8").unwrap();
        assert_eq!(
            variant_in(text, &base).unwrap().as_str(),
            "https://cdn.example.com/high.m3u8"
        );
    }

    #[test]
    fn media_playlists_have_no_variant() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:4\n#EXTINF:4,\nseg1.aac\n";
        let base = Url::parse("https://example.com/radio/index.m3u8").unwrap();
        assert!(variant_in(text, &base).is_none());
    }

    #[test]
    fn ignores_metadata() {
        let text = "Title=Something\nLength=1\n";
        assert!(stream_in(text, &base()).is_none());
    }
}
