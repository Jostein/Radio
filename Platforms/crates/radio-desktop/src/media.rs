//! Media controls: the system-level front end for the player.
//!
//! The tray menu is one way to drive the app; the desktop's own media widget is
//! the other. Both end up as the same actions, so the two can never disagree.
//!
//! On Linux this is MPRIS (see `mpris.rs`), which Plasma's media widget, GNOME's
//! Quick Settings, Cinnamon's and Budgie's sound applets, XFCE's mpris plugin,
//! waybar's `mpris` module and `playerctl` all speak. Windows has its own
//! mechanism (SMTC), so the platform-specific half stays behind this file.

use crate::player::{Player, State};

/// Playback as the desktop's media widget understands it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Playback {
    Stopped,
    Playing,
    Paused,
}

impl Playback {
    // Only MPRIS renders the name; the other backends use their own enums.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn as_str(self) -> &'static str {
        match self {
            Playback::Stopped => "Stopped",
            Playback::Playing => "Playing",
            Playback::Paused => "Paused",
        }
    }
}

/// What the media widget should show right now.
#[derive(Clone, PartialEq, Debug)]
pub struct Snapshot {
    pub playback: Playback,
    pub station: Option<String>,
    pub track: Option<String>,
    pub volume: f64,
}

impl Snapshot {
    pub fn from_player(player: &Player, volume: f64) -> Snapshot {
        let playback = match player.state {
            State::Playing => Playback::Playing,
            State::Paused => Playback::Paused,
            _ => Playback::Stopped,
        };

        Snapshot {
            playback,
            station: player.current.as_ref().map(|station| station.title.clone()),
            track: player.title.clone(),
            volume,
        }
    }
}

/// Something the media widget asked for.
///
/// Play and Pause are deliberately distinct: a media widget that sends `Play`
/// means "start", never "toggle", and a stream that pauses itself because the
/// user pressed play is the classic MPRIS bug.
// Each backend builds the subset it can honour: SMTC has no volume control and
// no quit, so those variants are only constructed by the Linux side.
#[derive(Clone, Debug)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum Action {
    Play,
    Pause,
    PlayPause,
    /// For a live stream this is the same as pausing; there is no end to stop at.
    Stop,
    Volume(f64),
    Quit,
}

/// The system media front end for this platform, if it has one.
pub struct Controls {
    inner: Option<imp::Service>,
}

impl Controls {
    /// Publishing media controls must never keep the app from starting: a
    /// desktop that cannot host them still gets the tray and its audio.
    pub fn start(snapshot: Snapshot) -> Controls {
        match imp::Service::start(snapshot) {
            Ok(service) => Controls {
                inner: Some(service),
            },
            Err(message) => {
                eprintln!("radio: media controls are off: {message}");
                Controls { inner: None }
            }
        }
    }

    /// Cheap enough for the app loop: it signals only what changed.
    pub fn update(&mut self, snapshot: Snapshot) {
        if let Some(service) = self.inner.as_mut() {
            service.update(snapshot);
        }
    }

    /// Actions requested since the last call.
    pub fn poll(&self) -> Vec<Action> {
        self.inner
            .as_ref()
            .map(imp::Service::poll)
            .unwrap_or_default()
    }
}

#[cfg(target_os = "linux")]
mod imp {
    pub use crate::mpris::MprisService as Service;
}

#[cfg(target_os = "windows")]
mod imp {
    pub use crate::smtc::SmtcService as Service;
}

/// Anywhere else the tray is the only way in; the app still builds and runs.
#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod imp {
    use super::{Action, Snapshot};

    pub struct Service;

    impl Service {
        pub fn start(_snapshot: Snapshot) -> Result<Service, String> {
            Ok(Service)
        }

        pub fn update(&mut self, _snapshot: Snapshot) {}

        pub fn poll(&self) -> Vec<Action> {
            Vec::new()
        }
    }
}
