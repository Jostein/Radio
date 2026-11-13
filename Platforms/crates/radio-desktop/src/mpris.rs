//! MPRIS: the freedesktop media player interface, which is how the app becomes
//! "the machine's radio" instead of another tray icon.
//!
//! One implementation covers every desktop, because they all consume this
//! interface: Plasma's media widget, GNOME's Quick Settings controls, Cinnamon's
//! and Budgie's sound applets, XFCE's mpris plugin, waybar's `mpris` module and
//! `playerctl`.
//!
//! Only the controls a live stream can honour do anything: play, pause, stop and
//! the volume property. There is nothing to seek and no station order to step
//! through, so those methods are deliberately inert rather than lying about
//! `CanSeek` and `CanGoNext`.

use crate::media::{Action, Snapshot};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, OwnedValue, Value};

const PATH: &str = "/org/mpris/MediaPlayer2";
const NAME: &str = "org.mpris.MediaPlayer2.radio";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const TRACK_ID: &str = "/radio/track";

/// MPRIS metadata for the current state. A live stream has no length and no
/// artwork of its own, so they are left out rather than faked.
fn metadata(snapshot: &Snapshot) -> HashMap<String, OwnedValue> {
    let mut metadata = HashMap::new();

    if let Some(title) = snapshot.track.as_ref().or(snapshot.station.as_ref()) {
        if let Ok(value) = OwnedValue::try_from(Value::from(title.as_str())) {
            metadata.insert("xesam:title".to_string(), value);
        }
    }

    // Widgets show the artist line above the title, and for radio the station is
    // exactly that. The spec wants a list of strings here, not a string.
    if let Some(station) = &snapshot.station {
        if let Ok(artists) = OwnedValue::try_from(Value::from(vec![station.clone()])) {
            metadata.insert("xesam:artist".to_string(), artists);
        }
    }

    // Some widget hosts compare track ids to decide whether the track changed.
    if let Ok(path) = ObjectPath::try_from(TRACK_ID) {
        if let Ok(id) = OwnedValue::try_from(Value::from(path)) {
            metadata.insert("mpris:trackid".to_string(), id);
        }
    }

    metadata
}

struct Shared {
    state: Mutex<Snapshot>,
    actions: Sender<Action>,
}

impl Shared {
    fn snapshot(&self) -> Snapshot {
        match self.state.lock() {
            Ok(state) => state.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn send(&self, action: Action) {
        let _ = self.actions.send(action);
    }
}

struct Root {
    shared: Arc<Shared>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    #[zbus(property)]
    fn identity(&self) -> String {
        "Radio".to_string()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "radio".to_string()
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_set_fullscreen(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["http".to_string(), "https".to_string()]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        Vec::new()
    }

    /// No window to bring forward.
    fn raise(&self) {}

    fn quit(&self) {
        self.shared.send(Action::Quit);
    }
}

struct MediaPlayer {
    shared: Arc<Shared>,
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl MediaPlayer {
    #[zbus(property)]
    fn playback_status(&self) -> String {
        self.shared.snapshot().playback.as_str().to_string()
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, OwnedValue> {
        metadata(&self.shared.snapshot())
    }

    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.shared.snapshot().volume
    }

    #[zbus(property)]
    fn set_volume(&self, volume: f64) -> zbus::fdo::Result<()> {
        self.shared.send(Action::Volume(volume.clamp(0.0, 1.0)));
        Ok(())
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn set_rate(&self, _rate: f64) -> zbus::fdo::Result<()> {
        Ok(())
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        1.0
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        "None".to_string()
    }

    #[zbus(property)]
    fn set_loop_status(&self, _status: String) -> zbus::fdo::Result<()> {
        Ok(())
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn set_shuffle(&self, _shuffle: bool) -> zbus::fdo::Result<()> {
        Ok(())
    }

    /// A live stream has no position to report.
    #[zbus(property)]
    fn position(&self) -> i64 {
        0
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        self.shared.snapshot().station.is_some()
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        false
    }

    fn play(&self) {
        self.shared.send(Action::Play);
    }

    fn pause(&self) {
        self.shared.send(Action::Pause);
    }

    fn play_pause(&self) {
        self.shared.send(Action::PlayPause);
    }

    /// There is no end of stream to stop at, so this pauses and reports Paused.
    fn stop(&self) {
        self.shared.send(Action::Stop);
    }

    fn next(&self) {}

    fn previous(&self) {}

    fn seek(&self, _offset: i64) {}

    fn set_position(&self, _track_id: ObjectPath<'_>, _position: i64) {}

    fn open_uri(&self, _uri: String) {}
}

/// A published MPRIS service. Dropping it withdraws the name from the bus.
pub struct MprisService {
    connection: Connection,
    shared: Arc<Shared>,
    actions: Receiver<Action>,
    last: Snapshot,
}

impl MprisService {
    pub fn start(snapshot: Snapshot) -> Result<MprisService, String> {
        let (sender, actions) = mpsc::channel();
        let shared = Arc::new(Shared {
            state: Mutex::new(snapshot.clone()),
            actions: sender,
        });

        let connection = Connection::session()
            .map_err(|error| format!("the session bus is not reachable ({error})"))?;

        connection
            .request_name(NAME)
            .map_err(|error| format!("{NAME} is already taken ({error})"))?;

        {
            let server = connection.object_server();
            server
                .at(
                    PATH,
                    Root {
                        shared: shared.clone(),
                    },
                )
                .map_err(|error| format!("the MPRIS root interface was rejected ({error})"))?;
            server
                .at(
                    PATH,
                    MediaPlayer {
                        shared: shared.clone(),
                    },
                )
                .map_err(|error| format!("the MPRIS player interface was rejected ({error})"))?;
        }

        Ok(MprisService {
            connection,
            shared,
            actions,
            last: snapshot,
        })
    }

    /// Publishes a new state, signalling only what changed. Called from the app
    /// loop, so it stays cheap.
    pub fn update(&mut self, snapshot: Snapshot) {
        if snapshot == self.last {
            return;
        }

        if let Ok(mut state) = self.shared.state.lock() {
            *state = snapshot.clone();
        }

        let mut changed: HashMap<&'static str, Value<'static>> = HashMap::new();
        if snapshot.playback != self.last.playback {
            changed.insert("PlaybackStatus", Value::from(snapshot.playback.as_str()));
        }
        if snapshot.station != self.last.station || snapshot.track != self.last.track {
            changed.insert("Metadata", Value::from(metadata(&snapshot)));
        }
        if (snapshot.volume - self.last.volume).abs() > f64::EPSILON {
            changed.insert("Volume", Value::from(snapshot.volume));
        }
        if snapshot.station.is_some() != self.last.station.is_some() {
            changed.insert("CanPlay", Value::from(snapshot.station.is_some()));
        }

        self.last = snapshot;

        if changed.is_empty() {
            return;
        }

        let invalidated: Vec<String> = Vec::new();
        let _ = self.connection.emit_signal(
            None::<&str>,
            PATH,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &(PLAYER_INTERFACE, changed, invalidated),
        );

    }

    /// Actions the media widget asked for since the last call.
    pub fn poll(&self) -> Vec<Action> {
        self.actions.try_iter().collect()
    }
}
