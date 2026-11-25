use crate::icon;
use crate::media::{self, Controls, Snapshot};
use crate::menu::{self, Handles};
use crate::player::{Player, State};
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
    /// The Windows flyout a left click opens. `None` when the window could not be
    /// created, in which case the menu is the only front end, as before.
    #[cfg(target_os = "windows")]
    panel: Option<crate::flyout::Panel>,
    last_header: String,
    last_track: String,
    last_toggle: String,
    last_volume: String,
    last_tooltip: String,
    last_look: icon::Look,
    last_current: Option<String>,
    saved_mtime: Option<SystemTime>,
    last_file_check: Instant,
    /// The wall clock at the previous turn, so a machine that was asleep can be
    /// told from one that was merely idle.
    last_tick: SystemTime,
}

impl App {
    pub fn new() -> Result<App, String> {
        let store = Store::load();
        let volume = store.volume as f32;
        let audio = crate::audio::Audio::start_or_silent(volume);
        let player = Player::new(audio, volume);

        let built = menu::build(&store, &player);
        let builder = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(built.menu))
            .with_tooltip("Radio")
            .with_icon(icon::tray_icon(icon::Look::Idle)?);
        // On Windows a left click opens the flyout and the right button opens
        // this menu; the crate shows the menu on right click either way. The
        // other platforms keep the menu on both, so they are left alone.
        #[cfg(target_os = "windows")]
        let builder = builder.with_menu_on_left_click(false);
        let tray = builder
            .build()
            .map_err(|error| format!("The tray icon could not be created: {error}"))?;

        let saved_mtime = store.modified();
        let media = Controls::start(Snapshot::from_player(&player, store.volume));

        // A panel that cannot be made must not keep the app from starting: the
        // menu still does everything, so this is reported and walked past.
        #[cfg(target_os = "windows")]
        let panel = crate::flyout::Panel::new();
        #[cfg(target_os = "windows")]
        if panel.is_none() {
            eprintln!("radio: the flyout panel is off; the tray menu still works");
        }

        let mut app = App {
            store,
            player,
            tray,
            handles: built.handles,
            media,
            #[cfg(target_os = "windows")]
            panel,
            last_header: String::new(),
            last_track: String::new(),
            last_toggle: String::new(),
            last_volume: String::new(),
            last_tooltip: String::new(),
            last_look: icon::Look::Idle,
            last_current: None,
            saved_mtime,
            last_file_check: Instant::now(),
            last_tick: std::time::SystemTime::now(),
        };
        app.sync();

        // The prototype's measurements have no mouse to click the tray icon
        // with, so they ask for the panel to be open from the start. A snapshot
        // path additionally draws it into a bitmap and quits: a worker without
        // a screen is looked at through that file, never through Remote Desktop.
        #[cfg(target_os = "windows")]
        if std::env::var_os("RADIO_FLYOUT_DEMO").is_some() {
            let snapshot = std::env::var_os("RADIO_FLYOUT_SNAPSHOT");
            let content = if snapshot.is_some() {
                app.preview_content()
            } else {
                app.panel_content()
            };
            if let Some(panel) = app.panel.as_mut() {
                panel.demo(content);
            }
            if let Some(path) = snapshot {
                let path = std::path::PathBuf::from(path);
                match app.panel.as_ref().map(|panel| panel.snapshot(&path)) {
                    Some(Ok(())) => eprintln!("radio: drew the panel into {}", path.display()),
                    Some(Err(error)) => {
                        eprintln!("radio: could not draw {}: {error}", path.display())
                    }
                    None => eprintln!("radio: the flyout panel is off, so nothing to draw"),
                }
                std::process::exit(0);
            }
        }

        Ok(app)
    }

    /// One turn of the app loop. Cheap; called every few tens of milliseconds.
    pub fn pump(&mut self) {
        self.recover_after_wake();
        self.handle_menu_events();
        self.handle_media();
        #[cfg(target_os = "windows")]
        self.handle_tray_clicks();
        self.player.pump();
        self.sync();
        self.watch_store_file();
        // Last, so the panel is handed the state this turn produced.
        #[cfg(target_os = "windows")]
        self.handle_panel();
    }

    /// Sleep freezes the socket, the sample counter and the polling loop, so a
    /// turn that arrives much later than the loop's own interval means the
    /// machine was away. Tune in again rather than trusting a connection that
    /// sat idle for hours, which is what the macOS app does on `didWake`.
    fn recover_after_wake(&mut self) {
        let now = SystemTime::now();
        let away = now
            .duration_since(self.last_tick)
            .map(|gap| gap >= Duration::from_secs(30))
            .unwrap_or(true);
        self.last_tick = now;

        if away {
            self.player.rejoin();
        }
    }

    fn handle_menu_events(&mut self) {        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id.0.clone();

            match id.as_str() {
                menu::TOGGLE => self.player.toggle(),
                menu::VOLUME_UP => self.change_volume(0.1),
                menu::VOLUME_DOWN => self.change_volume(-0.1),
                menu::YOUTUBE => self.open_track_search(),
                menu::ADD_STATION => {
                    let answer = crate::dialog::add_station();
                    // A dialog owns this thread while it is open, so the next
                    // turn can arrive minutes later. That is not a wake, and
                    // must not restart what is playing.
                    self.last_tick = SystemTime::now();
                    if let Some(station) = answer {
                        let known = self
                            .store
                            .stations
                            .iter()
                            .find(|saved| saved.url_string == station.url_string)
                            .cloned();
                        match known {
                            Some(saved) => self.player.play(saved),
                            None => {
                                self.store.add(station.clone());
                                self.rebuild();
                                self.player.play(station);
                            }
                        }
                    }
                }
                #[cfg(target_os = "windows")]
                menu::FIND_STATIONS => {
                    let answer = crate::find::find_station();
                    self.last_tick = SystemTime::now();
                    if let Some(station) = answer {
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
                            let current = self.player.current.as_ref().map(|s| s.id.as_str());
                            if current == Some(station.id.as_str()) {
                                self.player.toggle();
                            } else {
                                self.player.play(station);
                            }
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

    /// A left click on the tray icon opens or closes the flyout. The menu stays on
    /// the right button, where `tray-icon` shows it whatever we do here.
    #[cfg(target_os = "windows")]
    fn handle_tray_clicks(&mut self) {
        use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                rect,
                ..
            } = event
            {
                let anchor = crate::flyout::Anchor {
                    x: rect.position.x as i32,
                    y: rect.position.y as i32,
                    width: rect.size.width as i32,
                };
                let content = self.panel_content();
                if let Some(panel) = self.panel.as_mut() {
                    panel.toggle(content, anchor);
                }
            }
        }
    }

    /// Applies whatever the panel sent, then hands it the state that resulted.
    #[cfg(target_os = "windows")]
    fn handle_panel(&mut self) {
        let actions = match self.panel.as_mut() {
            Some(panel) => panel.take_actions(),
            None => return,
        };

        for action in actions {
            match action {
                crate::flyout::Action::Play(id) => {
                    if let Some(station) = self.store.station(&id).cloned() {
                        let current = self.player.current.as_ref().map(|s| s.id.as_str());
                        if current == Some(station.id.as_str()) {
                            self.player.toggle();
                        } else {
                            self.player.play(station);
                        }
                    }
                }
                crate::flyout::Action::Toggle => self.player.toggle(),
                crate::flyout::Action::Volume(volume) => {
                    self.store.set_volume(volume);
                    self.player.set_volume(volume as f32);
                }
            }
        }

        let content = self.panel_content();
        if let Some(panel) = self.panel.as_mut() {
            if panel.is_open() {
                panel.update(content);
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn panel_content(&self) -> crate::flyout::Content {
        crate::flyout::Content {
            title: self
                .player
                .current
                .as_ref()
                .map(|station| station.title.clone())
                .unwrap_or_default(),
            status: self.player.state.description(),
            detail: menu::track_text(&self.player),
            volume: self.store.volume,
            rows: self
                .store
                .stations
                .iter()
                .map(|station| crate::flyout::Row {
                    id: station.id.clone(),
                    title: station.title.clone(),
                })
                .collect(),
            current: self.player.current.as_ref().map(|station| station.id.clone()),
        }
    }

    /// What a snapshot draws. A build machine has no audio device and nothing
    /// ever starts playing on it, so the live content is one grey line and an
    /// empty list; this marks the store's first station as playing instead, so
    /// the picture is the panel a listener sees.
    #[cfg(target_os = "windows")]
    fn preview_content(&self) -> crate::flyout::Content {
        let mut content = self.panel_content();
        let Some(first) = self.store.stations.first() else {
            return content;
        };
        content.title = first.title.clone();
        content.status = crate::player::State::Playing.description();
        content.current = Some(first.id.clone());
        content
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
        let look = match self.player.state {
            State::Connecting | State::Playing => icon::Look::Playing,
            State::Reconnecting(_) => icon::Look::Reconnecting,
            State::Idle | State::Paused | State::Failed(_) => icon::Look::Idle,
        };
        if look != self.last_look {
            if let Ok(drawn) = icon::tray_icon(look) {
                let _ = self.tray.set_icon(Some(drawn));
            }
            self.last_look = look;
        }

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
