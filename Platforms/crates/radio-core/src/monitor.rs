use crate::feeds::{self, Feed};
use crate::icy::{self, TrackUpdate};
use std::io::Read;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};
use url::Url;

const ICY_REFRESH: Duration = Duration::from_secs(30);
const ICY_RETRY_DELAY: Duration = Duration::from_secs(6);
const ICY_RETRIES: u32 = 3;
const FEED_RETRY_DELAY: Duration = Duration::from_secs(5);
const FEED_RETRIES: u32 = 2;
const ICY_TIMEOUT: Duration = Duration::from_secs(8);
const FEED_TIMEOUT: Duration = Duration::from_secs(8);

/// What the UI should show.
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    Title(Option<String>),
}

enum Command {
    Start { url: String, generation: u64 },
    Stop { generation: u64, clear: bool },
    Discovered { generation: u64, feed: Option<Feed> },
}

/// The source currently being polled: the stream's own ICY blocks, or a feed.
#[derive(Clone)]
enum Source {
    Icy,
    Feed(Feed),
}

impl Source {
    fn refresh(&self) -> Duration {
        match self {
            Source::Icy => ICY_REFRESH,
            Source::Feed(feed) => feed.refresh,
        }
    }

    fn retry_delay(&self) -> Duration {
        match self {
            Source::Icy => ICY_RETRY_DELAY,
            Source::Feed(_) => FEED_RETRY_DELAY,
        }
    }

    fn retries(&self) -> u32 {
        match self {
            Source::Icy => ICY_RETRIES,
            Source::Feed(_) => FEED_RETRIES,
        }
    }
}

struct Active {
    generation: u64,
    url: Url,
    source: Source,
    retries_left: u32,
    next_at: Instant,
}

/// Supplies the current track title for a stream.
///
/// Mirrors the macOS monitor: a feed is preferred over reading the audio, a
/// feed that stops answering hands back to the stream, and a generation counter
/// keeps results from a replaced source out of the UI. One polling thread does
/// all of it.
pub struct Monitor {
    commands: Sender<Command>,
    updates: Receiver<Update>,
    generation: u64,
}

impl Monitor {
    pub fn spawn() -> Monitor {
        let (command_tx, command_rx) = mpsc::channel();
        let (update_tx, update_rx) = mpsc::channel();

        let discovery_sender = command_tx.clone();
        thread::Builder::new()
            .name("now-playing".to_string())
            .spawn(move || run(discovery_sender, command_rx, update_tx))
            .expect("the now playing thread could not start");

        Monitor {
            commands: command_tx,
            updates: update_rx,
            generation: 0,
        }
    }

    pub fn start(&mut self, url: &str) {
        self.generation += 1;
        let _ = self.commands.send(Command::Start {
            url: url.to_string(),
            generation: self.generation,
        });
    }

    /// `clear: false` keeps the last known track, so pausing leaves it visible.
    pub fn stop(&mut self, clear: bool) {
        self.generation += 1;
        let _ = self.commands.send(Command::Stop {
            generation: self.generation,
            clear,
        });
    }

    pub fn try_recv(&self) -> Option<Update> {
        self.updates.try_recv().ok()
    }
}

fn run(commands: Sender<Command>, incoming: Receiver<Command>, updates: Sender<Update>) {
    let mut active: Option<Active> = None;

    loop {
        let wait = active
            .as_ref()
            .map(|state| state.next_at.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(3600));

        match incoming.recv_timeout(wait) {
            Ok(Command::Start { url, generation }) => {
                active = start(&url, generation, &commands);
                if let Some(state) = active.as_mut() {
                    poll(state, &updates);
                }
            }
            Ok(Command::Stop { generation, clear }) => {
                match active.take() {
                    Some(state) if generation < state.generation => {
                        // A stop for an older stream must not end a newer one.
                        active = Some(state);
                    }
                    _ => {
                        if clear {
                            let _ = updates.send(Update::Title(None));
                        }
                    }
                }
            }
            Ok(Command::Discovered { generation, feed }) => {
                // A feed found for a stream the user already left is ignored.
                if let Some(state) = active.as_mut() {
                    if state.generation == generation {
                        if let Some(feed) = feed {
                            state.source = Source::Feed(feed);
                            state.retries_left = FEED_RETRIES;
                            state.next_at = Instant::now();
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some(state) = active.as_mut() {
                    poll(state, &updates);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn start(url: &str, generation: u64, commands: &Sender<Command>) -> Option<Active> {
    let parsed = Url::parse(url).ok()?;

    if let Some(feed) = feeds::curated(&parsed) {
        return Some(Active {
            generation,
            url: parsed,
            source: Source::Feed(feed),
            retries_left: FEED_RETRIES,
            next_at: Instant::now(),
        });
    }

    // Stream metadata starts immediately; discovery runs alongside it and can
    // only improve on the result.
    let discovery_commands = commands.clone();
    let discovery_url = parsed.clone();
    thread::Builder::new()
        .name("now-playing-discovery".to_string())
        .spawn(move || {
            let feed = feeds::discover(&discovery_url);
            let _ = discovery_commands.send(Command::Discovered { generation, feed });
        })
        .ok();

    Some(Active {
        generation,
        url: parsed,
        source: Source::Icy,
        retries_left: ICY_RETRIES,
        next_at: Instant::now(),
    })
}

fn poll(state: &mut Active, updates: &Sender<Update>) {
    let update = match &state.source {
        Source::Icy => icy::read_title(&state.url, ICY_TIMEOUT),
        Source::Feed(feed) => read_feed(feed),
    };

    match update {
        TrackUpdate::Title(title) => {
            let _ = updates.send(Update::Title(Some(title)));
            state.retries_left = state.source.retries();
            state.next_at = Instant::now() + state.source.refresh();
        }
        TrackUpdate::NoTitle => {
            // News, ads or a jingle: drop the last title instead of showing it.
            let _ = updates.send(Update::Title(None));
            state.retries_left = state.source.retries();
            state.next_at = Instant::now() + state.source.refresh();
        }
        TrackUpdate::Unavailable => {
            if state.retries_left > 0 {
                state.retries_left -= 1;
                state.next_at = Instant::now() + state.source.retry_delay();
            } else if matches!(&state.source, Source::Feed(_)) {
                // The feed is gone; the stream's own metadata takes over.
                state.source = Source::Icy;
                state.retries_left = ICY_RETRIES;
                state.next_at = Instant::now();
            } else {
                // ICY has nothing behind it, so wait for the next refresh.
                state.next_at = Instant::now() + ICY_REFRESH;
            }
        }
    }
}

fn read_feed(feed: &Feed) -> TrackUpdate {
    let response = match ureq::get(feed.url.as_str())
        .set("User-Agent", "Radio/1.0")
        .timeout(FEED_TIMEOUT)
        .call()
    {
        Ok(response) => response,
        Err(_) => return TrackUpdate::Unavailable,
    };

    let mut body = Vec::new();
    if response.into_reader().read_to_end(&mut body).is_err() {
        return TrackUpdate::Unavailable;
    }

    match feed.title(&body) {
        Some(title) if !title.trim().is_empty() => TrackUpdate::Title(title),
        _ => TrackUpdate::Unavailable,
    }
}
