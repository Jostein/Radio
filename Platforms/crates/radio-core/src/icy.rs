use std::io::Read;
use std::time::Duration;
use url::Url;

const USER_AGENT: &str = "Radio/1.0";

/// The answer to one attempt at learning what is playing.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackUpdate {
    Title(String),
    /// The source answered, but has no song right now: news, ads, a jingle, or
    /// a station that publishes nothing at all.
    NoTitle,
    /// The source could not be read this time: no metadata, a network error, or
    /// silence. The last known title stays on screen.
    Unavailable,
}

/// Reads `StreamTitle` from an Icecast/Shoutcast stream's inline metadata.
///
/// Metadata sits *inside* the audio stream at every `icy-metaint` bytes, so
/// reaching it means reading (and discarding) the audio before it. This reader
/// takes exactly one block and hangs up; the caller re-reads periodically
/// instead of keeping a second full copy of the stream open.
pub fn read_title(url: &Url, timeout: Duration) -> TrackUpdate {
    let response = match ureq::get(url.as_str())
        .set("Icy-MetaData", "1")
        .set("User-Agent", USER_AGENT)
        .timeout(timeout)
        .call()
    {
        Ok(response) => response,
        Err(_) => return TrackUpdate::Unavailable,
    };

    let metaint = response
        .header("icy-metaint")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0);

    let Some(metaint) = metaint else {
        // Not an ICY stream (HLS, plain file, error): nothing to read.
        return TrackUpdate::Unavailable;
    };

    let mut reader = response.into_reader();
    let mut audio = vec![0u8; metaint];
    if reader.read_exact(&mut audio).is_err() {
        return TrackUpdate::Unavailable;
    }

    let mut length = [0u8; 1];
    if reader.read_exact(&mut length).is_err() {
        return TrackUpdate::Unavailable;
    }

    let block_length = length[0] as usize * 16;
    if block_length == 0 {
        // An empty block is an answer: the station has no song right now.
        return TrackUpdate::NoTitle;
    }

    let mut block = vec![0u8; block_length];
    if reader.read_exact(&mut block).is_err() {
        return TrackUpdate::Unavailable;
    }

    match stream_title(&block) {
        Some(title) => TrackUpdate::Title(title),
        None => TrackUpdate::NoTitle,
    }
}

fn stream_title(block: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(block);
    let start = text.find("StreamTitle='")? + "StreamTitle='".len();
    let rest = &text[start..];
    let end = rest.find('\'')?;
    let title = rest[..end].trim();

    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_stream_title() {
        let block = b"StreamTitle='Dagny - Somebody';";
        assert_eq!(stream_title(block).as_deref(), Some("Dagny - Somebody"));
    }

    #[test]
    fn treats_an_empty_title_as_none() {
        let block = b"StreamTitle='';";
        assert_eq!(stream_title(block), None);
    }

    #[test]
    fn ignores_blocks_without_a_stream_title() {
        assert_eq!(stream_title(b"adw_ad=1;"), None);
    }
}
