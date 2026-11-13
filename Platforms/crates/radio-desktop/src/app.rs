use crate::icon;
use crate::media::{self, Controls, Snapshot};
use crate::menu::{self, Handles};
use crate::player::Player;
use radio_core::{search, store::Store};
use std::time::{Duration, Instant, SystemTime};
use tray_icon::menu::MenuEvent;
use tray_icon::TrayIcon;

pub struct App {
    store: Store,
    player: Player,
    tray: TrayIcon,
    handles: Handles,
    media: Controls,
    last_header: String,
    last_track: String,
    last_toggle: String,
    last_volume: String,
    last_tooltip: String,
    last_current: Option<String>,
    saved_mtime: Option<SystemTime>,
    last_file_check: Instant,
}

impl App {
    pub fn new() -> Result<App, String> {
        let store = Store::load();
        let volume = store.volume as f32;
        let audio = crate::audio::Audio::start(volume)?;
        let player = Player::new(audio, volume);

        let built = menu::build(&store, &player);
        let tray = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(built.menu))
            .with_tooltip("Radio")
            .with_icon(icon::tray_icon()?)
            .build()
            .map_err(|error| format!("The tray icon could not be created: {error}"))?;

        let saved_mtime = store.modified();
        let media = Controls::start(Snapshot::from_player(&player, store.volume));
        let mut app = App {
            store,
            player,
            tray,
            handles: built.handles,
            media,
            last_header: String::new(),
            last_track: String::new(),
            last_toggle: String::new(),
            last_volume: String::new(),
            last_tooltip: String::new(),
            last_current: None,
            saved_mtime,
            last_file_check: Instant::now(),
        };
        app.sync();
        Ok(app)
    }

    /// One turn of the app loop. Cheap; called every few tens of milliseconds.
    pub fn pump(&mut self) {
        self.handle_menu_events();
        self.handle_media();
        self.player.pump();
        self.sync();
        self.watch_store_file();
    }

    fn handle_menu_events(&mut self) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id.0.clone();

            match id.as_str() {
                menu::TOGGLE => self.player.toggle(),
                menu::VOLUME_UP => self.change_volume(0.1),
                menu::VOLUME_DOWN => self.change_volume(-0.1),
                menu::YOUTUBE => self.open_track_search(),
                menu::ADD_STATION => {
                    if let Some(station) = crate::dialog::add_station() {
                        self.store.add(station);
                        self.rebuild();
                    }
                }
                menu::OPEN_STATIONS => {
                    let _ = open::that(self.store.stations_file());
                }
                menu::RELOAD_STATIONS => {
                    self.store.reload();
                    self.rebuild();
                }
                menu::QUIT => {
                    self.player.stop();
                    std::process::exit(0);
                }
                other => {
                    if let Some(station_id) = other.strip_prefix("play:") {
                        if let Some(station) = self.store.station(station_id).cloned() {
                            self.player.play(station);
                        }
                    } else if let Some(station_id) = other.strip_prefix("remove:") {
                        if self.store.remove(station_id) {
                            if self
                                .player
                                .current
                                .as_ref()
                                .map(|station| station.id == station_id)
                                .unwrap_or(false)
                            {
                                self.player.stop();
                            }
                            self.rebuild();
                        }
                    }
                }
            }
        }
    }

    /// The desktop's media widget sends the same actions the menu does, so both
    /// front ends are handled in one place.
    fn handle_media(&mut self) {
        let actions: Vec<media::Action> = self.media.poll();
        for action in actions {
            match action {
                media::Action::Play => self.player.resume(),
                media::Action::Pause | media::Action::Stop => self.player.pause(),
                media::Action::PlayPause => self.player.toggle(),
                media::Action::Volume(volume) => {
                    self.store.set_volume(volume);
                    self.player.set_volume(volume as f32);
                }
                media::Action::Quit => {
                    self.player.stop();
                    std::process::exit(0);
                }
            }
        }
    }

    fn change_volume(&mut self, delta: f64) {
        let volume = (self.store.volume + delta).clamp(0.0, 1.0);
        self.store.set_volume(volume);
        self.player.set_volume(volume as f32);
    }

    fn open_track_search(&self) {
        let Some(title) = self.player.title.as_deref() else {
            return;
        };
        if let Some(url) = search::youtube_url(title) {
            let _ = open::that(url);
        }
    }

    /// Pushes changed text into the existing items. Rebuilding is reserved for
    /// changes in structure (the station list), which are rare.
    fn sync(&mut self) {
        let header = menu::header_text(&self.player);
        if header != self.last_header {
            self.handles.header.set_text(&header);
            self.last_header = header;
        }

        let track = menu::track_text(&self.player);
        if track != self.last_track {
            self.handles.track.set_text(&track);
            self.handles.track.set_enabled(self.player.title.is_some());
            self.last_track = track;
        }

        let toggle = menu::toggle_text(&self.player).to_string();
        if toggle != self.last_toggle {
            self.handles.toggle.set_text(&toggle);
            self.last_toggle = toggle;
        }
        self.handles.toggle.set_enabled(self.player.current.is_some());

        let volume = menu::volume_text(self.store.volume);
        if volume != self.last_volume {
            self.handles.volume.set_text(&volume);
            self.last_volume = volume;
        }

        let current_id = self.player.current.as_ref().map(|station| station.id.clone());
        if current_id != self.last_current {
            for item in &self.handles.stations {
                let id = item.id().0.clone();
                let station_id = id.strip_prefix("play:").unwrap_or("").to_string();
                item.set_checked(Some(&station_id) == current_id.as_ref());
            }
            self.last_current = current_id;
        }

        let tooltip = match (&self.player.current, &self.player.title) {
            (Some(station), Some(track)) => format!("Radio — {} — {track}", station.title),
            (Some(station), None) => format!("Radio — {} — {}", station.title, self.player.state.description()),
            _ => "Radio".to_string(),
        };
        if tooltip != self.last_tooltip {
            let _ = self.tray.set_tooltip(Some(tooltip.clone()));
            self.last_tooltip = tooltip;
        }

        self.media
            .update(Snapshot::from_player(&self.player, self.store.volume));
    }

    /// Stations can also be edited in the JSON file, so a change there is picked
    /// up the way the macOS app picks up its own store changes.
    fn watch_store_file(&mut self) {
        if self.last_file_check.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_file_check = Instant::now();

        let mtime = self.store.modified();
        if mtime.is_none() || mtime == self.saved_mtime {
            return;
        }

        self.saved_mtime = mtime;
        self.store.reload();
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let built = menu::build(&self.store, &self.player);
        self.tray.set_menu(Some(Box::new(built.menu)));
        self.handles = built.handles;
        self.last_header.clear();
        self.last_track.clear();
        self.last_toggle.clear();
        self.last_volume.clear();
        self.last_current = None;
        self.sync();
    }
}
