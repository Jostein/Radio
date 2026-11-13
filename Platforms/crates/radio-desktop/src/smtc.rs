//! SMTC: System Media Transport Controls, the Windows counterpart of MPRIS.
//!
//! This is what puts "Radio" in the media flyout, on the lock screen, and on the
//! keyboard's media keys. Unlike MPRIS it is not a bus protocol: the WinRT object
//! is bound to a top-level window of the process, so the app creates a hidden one
//! (see `platform.rs`) purely to own it.
//!
//! Only play, pause and stop are wired up, matching what a live stream can
//! honestly do: there is nothing to seek and no track list to step through.
//! Windows offers no volume control through SMTC, so the tray's Volume submenu
//! stays the only volume control here.

use crate::media::{Action, Playback, Snapshot};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use windows::core::{factory, HSTRING};
use windows::Foundation::TypedEventHandler;
use windows::Media::{
    MediaPlaybackStatus, MediaPlaybackType, SystemMediaTransportControls,
    SystemMediaTransportControlsButton, SystemMediaTransportControlsButtonPressedEventArgs,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::WinRT::{
    ISystemMediaTransportControlsInterop, RoInitialize, RO_INIT_MULTITHREADED,
};

/// The hidden window SMTC is attached to, published by `platform.rs` before the
/// app is built. Zero means "no window yet".
static WINDOW: AtomicIsize = AtomicIsize::new(0);

pub fn set_window(hwnd: isize) {
    WINDOW.store(hwnd, Ordering::Relaxed);
}

struct Shared {
    state: Mutex<Snapshot>,
    actions: Mutex<Sender<Action>>,
}

impl Shared {
    fn send(&self, action: Action) {
        if let Ok(actions) = self.actions.lock() {
            let _ = actions.send(action);
        }
    }
}

/// A published set of system media controls.
pub struct SmtcService {
    controls: SystemMediaTransportControls,
    shared: Arc<Shared>,
    actions: Receiver<Action>,
    last: Snapshot,
    handler_token: i64,
}

impl SmtcService {
    pub fn start(snapshot: Snapshot) -> Result<SmtcService, String> {
        let raw = WINDOW.load(Ordering::Relaxed);
        if raw == 0 {
            return Err("the hidden window for media controls was not created".to_string());
        }

        // Media events arrive on MTA threads, so the app loop does not have to
        // pump them; it collects the actions from the channel instead.
        unsafe {
            let _ = RoInitialize(RO_INIT_MULTITHREADED);
        }

        let interop: ISystemMediaTransportControlsInterop =
            factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()
                .map_err(|error| format!("the media controls could not be activated ({error})"))?;

        let hwnd = HWND(raw as *mut core::ffi::c_void);
        let controls: SystemMediaTransportControls = unsafe { interop.GetForWindow(hwnd) }
            .map_err(|error| format!("the media controls refused the window ({error})"))?;

        let (sender, actions) = mpsc::channel();
        let shared = Arc::new(Shared {
            state: Mutex::new(snapshot.clone()),
            actions: Mutex::new(sender),
        });

        let pressed_shared = shared.clone();
        let handler = TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(move |_sender, args| {
            if let Some(args) = args.as_ref() {
                if let Ok(button) = args.Button() {
                    let action = if button == SystemMediaTransportControlsButton::Play {
                        Some(Action::Play)
                    } else if button == SystemMediaTransportControlsButton::Pause {
                        Some(Action::Pause)
                    } else if button == SystemMediaTransportControlsButton::Stop {
                        Some(Action::Stop)
                    } else {
                        None
                    };
                    if let Some(action) = action {
                        pressed_shared.send(action);
                    }
                }
            }
            Ok(())
        });

        let handler_token = controls
            .ButtonPressed(&handler)
            .map_err(|error| format!("the media buttons could not be wired up ({error})"))?;

        // Which buttons exist at all. Play is enabled once a station is loaded.
        let _ = controls.SetIsEnabled(true);
        let _ = controls.SetIsPlayEnabled(false);
        let _ = controls.SetIsPauseEnabled(true);
        let _ = controls.SetIsStopEnabled(true);
        let _ = controls.SetIsNextEnabled(false);
        let _ = controls.SetIsPreviousEnabled(false);
        let _ = controls.SetIsFastForwardEnabled(false);
        let _ = controls.SetIsRewindEnabled(false);
        let _ = controls.SetPlaybackStatus(MediaPlaybackStatus::Stopped);

        Ok(SmtcService {
            controls,
            shared,
            actions,
            last: snapshot,
            handler_token,
        })
    }

    pub fn update(&mut self, snapshot: Snapshot) {
        if snapshot == self.last {
            return;
        }

        if let Ok(mut state) = self.shared.state.lock() {
            *state = snapshot.clone();
        }

        if snapshot.playback != self.last.playback {
            let status = match snapshot.playback {
                Playback::Playing => MediaPlaybackStatus::Playing,
                Playback::Paused => MediaPlaybackStatus::Paused,
                Playback::Stopped => MediaPlaybackStatus::Stopped,
            };
            let _ = self.controls.SetPlaybackStatus(status);
        }

        if snapshot.station != self.last.station || snapshot.track != self.last.track {
            let _ = self.publish_metadata(&snapshot);
        }

        if snapshot.station.is_some() != self.last.station.is_some() {
            let _ = self
                .controls
                .SetIsPlayEnabled(snapshot.station.is_some());
        }

        self.last = snapshot;
    }

    /// The flyout, the lock screen and the volume overlay all read this.
    fn publish_metadata(&self, snapshot: &Snapshot) -> windows::core::Result<()> {
        let updater = self.controls.DisplayUpdater()?;
        updater.SetType(MediaPlaybackType::Music)?;

        let music = updater.MusicProperties()?;
        if let Some(title) = snapshot.track.as_ref().or(snapshot.station.as_ref()) {
            music.SetTitle(&HSTRING::from(title.as_str()))?;
        }
        if let Some(station) = &snapshot.station {
            music.SetArtist(&HSTRING::from(station.as_str()))?;
        }

        updater.Update()
    }

    pub fn poll(&self) -> Vec<Action> {
        self.actions.try_iter().collect()
    }
}

impl Drop for SmtcService {
    fn drop(&mut self) {
        let _ = self.controls.RemoveButtonPressed(self.handler_token);
        let _ = self.controls.SetIsEnabled(false);
    }
}
