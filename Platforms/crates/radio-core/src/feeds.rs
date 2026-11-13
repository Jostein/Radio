use serde_json::Value;
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use url::Url;

const USER_AGENT: &str = "Radio/1.0";
/// An ICY read costs 8–64 KB, so a bigger document would be a downgrade.
const MAXIMUM_FEED_BYTES: usize = 16_384;
const PROBE_TIMEOUT: Duration = Duration::from_secs(6);

type Parser = Arc<dyn Fn(&[u8]) -> Option<String> + Send + Sync>;

/// A JSON document that names the current track.
///
/// Reading one costs a few hundred bytes to a few kilobytes, against the
/// `icy-metaint` block of audio a stream read needs, so a feed is both cheaper
/// and fresher than the stream it belongs to.
#[derive(Clone)]
pub struct Feed {
    pub url: Url,
    pub refresh: Duration,
    parse: Parser,
}

impl Feed {
    pub fn new(url: Url, refresh: Duration, parse: impl Fn(&[u8]) -> Option<String> + Send + Sync + 'static) -> Feed {
        Feed {
            url,
            refresh,
            parse: Arc::new(parse),
        }
    }

    pub fn title(&self, body: &[u8]) -> Option<String> {
        (self.parse)(body)
    }
}

/// Broadcasters whose endpoints are known and stable.
pub fn curated(stream_url: &Url) -> Option<Feed> {
    let host = stream_url.host_str()?.to_ascii_lowercase();

    if host.ends_with("somafm.com") {
        return soma_fm(stream_url);
    }
    if host.contains("radioparadise.com") {
        return radio_paradise(stream_url);
    }
    if host.contains("radiofrance.fr") && stream_url.as_str().to_ascii_lowercase().contains("fip") {
        return radio_france(stream_url);
    }
    None
}

// MARK: - SomaFM

/// `/groovesalad-128-mp3` is channel `groovesalad`.
fn soma_fm(stream_url: &Url) -> Option<Feed> {
    let mount = stream_url.path_segments()?.next()?;
    let channel = soma_channel(mount)?;
    let url = Url::parse(&format!("https://somafm.com/songs/{channel}.json")).ok()?;

    Some(Feed::new(url, Duration::from_secs(10), |body| {
        let root = json(body)?;
        let current = root.get("songs")?.as_array()?.first()?;
        join(
            current.get("artist").and_then(Value::as_str),
            current.get("title").and_then(Value::as_str),
        )
    }))
}

fn soma_channel(mount: &str) -> Option<String> {
    let mut parts: Vec<&str> = mount.split('-').collect();
    while let Some(last) = parts.last() {
        let lowered = last.to_ascii_lowercase();
        let numeric = !lowered.is_empty() && lowered.chars().all(|c| c.is_ascii_digit());
        let codec = lowered == "mp3" || lowered.starts_with("aac");
        if numeric || codec {
            parts.pop();
        } else {
            break;
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("-"))
    }
}

// MARK: - Radio Paradise

fn radio_paradise(stream_url: &Url) -> Option<Feed> {
    let path = stream_url.as_str().to_ascii_lowercase();
    let channel = if path.contains("mellow") {
        1
    } else if path.contains("rock") {
        2
    } else if path.contains("global") || path.contains("world") {
        3
    } else {
        0
    };

    let url = Url::parse(&format!(
        "https://api.radioparadise.com/api/now_playing?chan={channel}"
    ))
    .ok()?;

    Some(Feed::new(url, Duration::from_secs(10), |body| {
        let root = json(body)?;
        join(
            root.get("artist").and_then(Value::as_str),
            root.get("title").and_then(Value::as_str),
        )
    }))
}

// MARK: - Radio France (FIP family)

/// livemeta station ids for the FIP webradios, most specific slug first.
const FIP_STATIONS: &[(&str, u32)] = &[
    ("fipreggae", 71),
    ("fiprock", 64),
    ("fipjazz", 65),
    ("fipgroove", 66),
    ("fipelectro", 74),
    ("fipmetal", 77),
    ("fipnouveau", 70),
    ("fipworld", 69),
    ("fipmonde", 69),
];

fn radio_france(stream_url: &Url) -> Option<Feed> {
    let address = stream_url.as_str().to_ascii_lowercase();
    let id = FIP_STATIONS
        .iter()
        .find(|(slug, _)| address.contains(slug))
        .map(|(_, id)| *id)
        .unwrap_or(7);

    let url = Url::parse(&format!("https://api.radiofrance.fr/livemeta/pull/{id}")).ok()?;

    // Radio France spends a few kilobytes on the schedule around the song, so
    // this one is polled less often and still beats the audio-block read.
    Some(Feed::new(url, Duration::from_secs(20), |body| {
        let root = json(body)?;
        let level = root.get("levels")?.as_array()?.first()?;
        let items = level.get("items")?.as_array()?;
        let position = level.get("position")?.as_u64()? as usize;
        let identifier = items.get(position)?;
        let steps = root.get("steps")?;

        let step = match steps {
            Value::Object(table) => table.get(&identifier.to_string()),
            Value::Array(list) => identifier.as_u64().and_then(|index| list.get(index as usize)),
            _ => None,
        }?;

        join(
            step.get("authors").and_then(Value::as_str),
            step.get("title").and_then(Value::as_str),
        )
    }))
}

// MARK: - Discovery

/// Looks for a status document this server publishes. Two conventions exist:
/// Icecast answers on `/status-json.xsl` for the whole server, Shoutcast on
/// `/stats?json=1`. A feed is adopted only when it names a title for this mount
/// and is small enough that polling it beats reading the audio.
pub fn discover(stream_url: &Url) -> Option<Feed> {
    icecast_feed(stream_url).or_else(|| shoutcast_feed(stream_url))
}

fn icecast_feed(stream_url: &Url) -> Option<Feed> {
    let url = endpoint(stream_url, "/status-json.xsl", None)?;
    let mount = stream_url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or("")
        .to_ascii_lowercase();

    let body = fetch(&url)?;
    icecast_title(&body, &mount)?;

    Some(Feed::new(url, interval(body.len()), move |body| {
        icecast_title(body, &mount)
    }))
}

fn icecast_title(body: &[u8], mount: &str) -> Option<String> {
    let root = json(body)?;
    let source = root.get("icestats")?.get("source")?;

    let sources: Vec<&Value> = match source {
        Value::Array(list) => list.iter().collect(),
        other => vec![other],
    };

    for entry in &sources {
        let listen = entry
            .get("listenurl")
            .and_then(Value::as_str)
            .and_then(|value| Url::parse(value).ok())
            .and_then(|url| {
                url.path_segments()
                    .and_then(|mut segments| segments.next_back())
                    .map(|mount| mount.to_ascii_lowercase())
            });

        if listen.as_deref() != Some(mount) {
            continue;
        }
        if let Some(title) = clean(entry.get("title").and_then(Value::as_str)) {
            return Some(title);
        }
    }

    // A server with exactly one mount is this station, whatever it calls it.
    if sources.len() == 1 {
        return clean(sources[0].get("title").and_then(Value::as_str));
    }
    None
}

fn shoutcast_feed(stream_url: &Url) -> Option<Feed> {
    let url = endpoint(stream_url, "/stats", Some("json=1"))?;

    let body = fetch(&url)?;
    shoutcast_title(&body)?;

    Some(Feed::new(url, interval(body.len()), shoutcast_title))
}

fn shoutcast_title(body: &[u8]) -> Option<String> {
    let root = json(body)?;
    for key in ["songtitle", "currentsong", "title"] {
        if let Some(title) = clean(root.get(key).and_then(Value::as_str)) {
            return Some(title);
        }
    }
    None
}

// MARK: - Plumbing

fn endpoint(stream_url: &Url, path: &str, query: Option<&str>) -> Option<Url> {
    let mut url = stream_url.clone();
    url.set_path(path);
    url.set_query(query);
    url.set_fragment(None);
    Some(url)
}

/// One bounded fetch: stops reading as soon as the document is too big to be
/// worth polling, so a 66 KB server status never gets downloaded in full.
fn fetch(url: &Url) -> Option<Vec<u8>> {
    let response = ureq::get(url.as_str())
        .set("User-Agent", USER_AGENT)
        .timeout(PROBE_TIMEOUT)
        .call()
        .ok()?;

    let mut reader = response.into_reader();
    let mut body = Vec::new();
    let mut chunk = [0u8; 4096];

    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                if body.len() + count > MAXIMUM_FEED_BYTES {
                    return None;
                }
                body.extend_from_slice(&chunk[..count]);
            }
            Err(_) => return None,
        }
    }

    Some(body)
}

fn interval(bytes: usize) -> Duration {
    if bytes <= 4096 {
        Duration::from_secs(10)
    } else {
        Duration::from_secs(20)
    }
}

fn json(body: &[u8]) -> Option<Value> {
    serde_json::from_slice(body).ok()
}

fn clean(value: Option<&str>) -> Option<String> {
    let trimmed = value.unwrap_or("").trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn join(artist: Option<&str>, title: Option<&str>) -> Option<String> {
    let artist = artist.unwrap_or("").trim();
    let title = title.unwrap_or("").trim();

    match (artist.is_empty(), title.is_empty()) {
        (false, false) => {
            if artist == title {
                Some(title.to_string())
            } else {
                Some(format!("{artist} — {title}"))
            }
        }
        (false, true) => Some(artist.to_string()),
        (true, false) => Some(title.to_string()),
        (true, true) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_somafm_mount_suffix() {
        assert_eq!(soma_channel("groovesalad-128-mp3").as_deref(), Some("groovesalad"));
        assert_eq!(soma_channel("deepspaceone-64-aac").as_deref(), Some("deepspaceone"));
        assert_eq!(soma_channel("beatblender").as_deref(), Some("beatblender"));
    }

    #[test]
    fn reads_icecast_status_for_the_mount() {
        let body = br#"{"icestats":{"source":[
            {"listenurl":"https://example.com/other","title":"Wrong"},
            {"listenurl":"https://example.com/stream","title":"Right"}
        ]}}"#;
        assert_eq!(icecast_title(body, "stream").as_deref(), Some("Right"));
    }

    #[test]
    fn joins_artist_and_title_once() {
        assert_eq!(join(Some("A"), Some("B")).as_deref(), Some("A — B"));
        assert_eq!(join(Some("A"), Some("A")).as_deref(), Some("A"));
        assert_eq!(join(None, Some("B")).as_deref(), Some("B"));
        assert_eq!(join(None, None), None);
    }
}
