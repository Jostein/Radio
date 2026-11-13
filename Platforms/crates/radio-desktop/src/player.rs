use crate::audio::{self, Audio};
use radio_core::{monitor::Update, playlist, station::Station, Monitor};
use std::process::Child;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

pub const MAX_RECONNECT_ATTEMPTS: u32 = 6;

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Connecting,
    Playing,
    Paused,
    Reconnecting(u32),
    Failed(String),
}

impl State {
    pub fn description(&self) -> String {
        match self {
            State::Idle => "Nothing playing".to_string(),
            State::Connecting => "Connecting…".to_string(),
            State::Playing => "Live".to_string(),
            State::Paused => "Paused".to_string(),
            State::Reconnecting(attempt) => {
                format!("Reconnecting ({attempt} of {MAX_RECONNECT_ATTEMPTS})…")
            }
            State::Failed(message) => message.clone(),
        }
    }
}

enum Worker {
    Ready {
        generation: u64,
        child: Child,
        url: String,
    },
    Failed {
        generation: u64,
        message: String,
    },
}

pub struct Player {
    audio: Audio,
    monitor: Monitor,
    worker_tx: Sender<Worker>,
    worker_rx: Receiver<Worker>,
    pub state: State,
    pub current: Option<Station>,
    pub title: Option<String>,
    child: Option<Child>,
    generation: u64,
    reconnect_attempt: u32,
    retry_at: Option<Instant>,
    stalled_ticks: u8,
    last_pushed: u64,
    last_watch: Instant,
    volume: f32,
}

impl Player {
    pub fn new(audio: Audio, volume: f32) -> Player {
        let (worker_tx, worker_rx) = mpsc::channel();
        audio.set_volume(volume);

        Player {
            audio,
            monitor: Monitor::spawn(),
            worker_tx,
            worker_rx,
            state: State::Idle,
            current: None,
            title: None,
            child: None,
            generation: 0,
            reconnect_attempt: 0,
            retry_at: None,
            stalled_ticks: 0,
            last_pushed: 0,
            last_watch: Instant::now(),
            volume,
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            State::Connecting | State::Playing | State::Reconnecting(_)
        )
    }

    pub fn play(&mut self, station: Station) {
        self.generation += 1;
        self.monitor.stop(true);
        self.stop_child();
        self.title = None;
        self.current = Some(station);
        self.reconnect_attempt = 0;
        self.retry_at = None;
        self.begin();
    }

    pub fn toggle(&mut self) {
        if self.is_active() {
            self.pause();
        } else {
            self.resume();
        }
    }

    /// Starts the current station unless it is already playing. Called by the
    /// media widget's `Play`, which must never pause.
    pub fn resume(&mut self) {
        if self.is_active() {
            return;
        }
        self.reconnect_attempt = 0;
        self.begin();
    }

    pub fn pause(&mut self) {
        // Bump the generation so a connection that lands after the click cannot
        // flip the app back into a playing state.
        self.generation += 1;
        self.monitor.stop(false);
        self.stop_child();
        self.state = if self.current.is_some() {
            State::Paused
        } else {
            State::Idle
        };
    }

    pub fn stop(&mut self) {
        self.generation += 1;
        self.monitor.stop(true);
        self.stop_child();
        self.current = None;
        self.title = None;
        self.reconnect_attempt = 0;
        self.retry_at = None;
        self.state = State::Idle;
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.audio.set_volume(self.volume);
    }

    pub fn pump(&mut self) {
        while let Ok(event) = self.worker_rx.try_recv() {
            match event {
                Worker::Ready {
                    generation,
                    mut child,
                    url,
                } if generation == self.generation => {
                    self.audio.pump_child(&mut child);
                    self.monitor.start(&url);
                    self.child = Some(child);
                    self.state = State::Playing;
                    self.stalled_ticks = 0;
                    self.last_pushed = 0;
                }
                Worker::Failed {
                    generation,
                    message,
                } if generation == self.generation => {
                    self.handle_failure(message);
                }
                _ => {}
            }
        }

        while let Some(update) = self.monitor.try_recv() {
            match update {
                Update::Title(title) => self.title = title,
            }
        }

        self.watch();
    }

    // MARK: - Internals

    fn begin(&mut self) {
        let Some(station) = self.current.clone() else {
            return;
        };

        self.generation += 1;
        let generation = self.generation;

        self.monitor.stop(true);
        self.stop_child();
        self.title = None;
        self.stalled_ticks = 0;
        self.state = if self.reconnect_attempt == 0 {
            State::Connecting
        } else {
            State::Reconnecting(self.reconnect_attempt)
        };

        let sender = self.worker_tx.clone();
        let sample_rate = self.audio.sample_rate;
        let channels = self.audio.channels;

        thread::Builder::new()
            .name("tune-in".to_string())
            .spawn(move || {
                let Some(url) = station.stream_url() else {
                    let _ = sender.send(Worker::Failed {
                        generation,
                        message: "That stream URL is not valid.".to_string(),
                    });
                    return;
                };

                let resolved = if playlist::is_playlist(&url) {
                    playlist::resolve(&url)
                } else {
                    Ok(url)
                };

                match resolved {
                    Ok(stream) => match audio::spawn_ffmpeg(stream.as_str(), sample_rate, channels) {
                        Ok(child) => {
                            let _ = sender.send(Worker::Ready {
                                generation,
                                child,
                                url: stream.to_string(),
                            });
                        }
                        Err(message) => {
                            let _ = sender.send(Worker::Failed { generation, message });
                        }
                    },
                    Err(message) => {
                        let _ = sender.send(Worker::Failed { generation, message });
                    }
                }
            })
            .ok();
    }

    fn stop_child(&mut self) {
        self.audio.silence();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Runs about once a second: notices a dead or silent stream and starts the
    /// next reconnect attempt when its backoff has elapsed.
    fn watch(&mut self) {
        if self.last_watch.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_watch = Instant::now();

        if let State::Reconnecting(_) = self.state {
            if let Some(at) = self.retry_at {
                if Instant::now() >= at {
                    self.retry_at = None;
                    self.begin();
                }
            }
            return;
        }

        if self.state != State::Playing {
            return;
        }

        if let Some(child) = self.child.as_mut() {
            if let Ok(Some(_)) = child.try_wait() {
                self.handle_failure("The stream stopped unexpectedly.".to_string());
                return;
            }
        }

        // A live stream that stops delivering samples is stuck. The fill level is
        // not a signal: a device that drains eagerly keeps the buffer empty while
        // playing fine, so this watches the sample counter instead.
        let pushed = self.audio.pushed();
        if pushed == self.last_pushed {
            self.stalled_ticks += 1;
            if self.stalled_ticks >= 4 {
                self.handle_failure("The stream stopped sending audio.".to_string());
            }
        } else {
            self.last_pushed = pushed;
            self.stalled_ticks = 0;
        }
    }

    fn handle_failure(&mut self, message: String) {
        self.stop_child();
        self.monitor.stop(true);
        self.title = None;

        if self.current.is_none() || self.reconnect_attempt >= MAX_RECONNECT_ATTEMPTS {
            self.state = State::Failed(message);
            self.retry_at = None;
            return;
        }

        self.reconnect_attempt += 1;
        self.state = State::Reconnecting(self.reconnect_attempt);

        let exponent = self.reconnect_attempt.saturating_sub(1);
        let delay = Duration::from_secs(2u64.pow(exponent).min(30));
        self.retry_at = Some(Instant::now() + delay);
    }
}
