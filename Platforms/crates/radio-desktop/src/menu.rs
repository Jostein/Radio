use crate::player::Player;
use radio_core::store::Store;
use radio_core::title;
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};

pub const TOGGLE: &str = "toggle";
pub const VOLUME_UP: &str = "vol:+";
pub const VOLUME_DOWN: &str = "vol:-";
pub const YOUTUBE: &str = "youtube";
pub const OPEN_STATIONS: &str = "open-stations";
pub const RELOAD_STATIONS: &str = "reload-stations";
pub const QUIT: &str = "quit";

/// Menu items whose text changes while the menu is closed, kept so they can be
/// updated in place instead of rebuilding the whole menu.
pub struct Handles {
    pub header: MenuItem,
    pub track: MenuItem,
    pub toggle: MenuItem,
    pub volume: MenuItem,
    pub stations: Vec<CheckMenuItem>,
}

pub struct Built {
    pub menu: Menu,
    pub handles: Handles,
}

pub fn build(store: &Store, player: &Player) -> Built {
    let menu = Menu::new();

    let header = MenuItem::new(header_text(player), false, None);
    let track = MenuItem::with_id(YOUTUBE, track_text(player), player.title.is_some(), None);
    let toggle = MenuItem::with_id(
        TOGGLE,
        toggle_text(player),
        player.current.is_some(),
        None,
    );

    let volume = MenuItem::new(volume_text(store.volume), false, None);
    let volume_menu = Submenu::new("Volume", true);
    let _ = volume_menu.append(&volume);
    let _ = volume_menu.append(&PredefinedMenuItem::separator());
    let louder = MenuItem::with_id(VOLUME_UP, "Louder", true, None);
    let quieter = MenuItem::with_id(VOLUME_DOWN, "Quieter", true, None);
    let _ = volume_menu.append(&louder);
    let _ = volume_menu.append(&quieter);

    let stations_menu = Submenu::new("Stations", true);
    let mut station_items = Vec::new();
    if store.stations.is_empty() {
        let _ = stations_menu.append(&MenuItem::new("No stations yet", false, None));
    } else {
        for station in &store.stations {
            let checked = player
                .current
                .as_ref()
                .map(|current| current.id == station.id)
                .unwrap_or(false);
            let item = CheckMenuItem::with_id(
                format!("play:{}", station.id),
                &station.title,
                true,
                checked,
                None,
            );
            let _ = stations_menu.append(&item);
            station_items.push(item);
        }
    }

    let remove_menu = Submenu::new("Remove Station", !store.stations.is_empty());
    for station in &store.stations {
        let item = MenuItem::with_id(format!("remove:{}", station.id), &station.title, true, None);
        let _ = remove_menu.append(&item);
    }

    let _ = menu.append(&header);
    let _ = menu.append(&track);
    let _ = menu.append(&toggle);
    let _ = menu.append(&volume_menu);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&stations_menu);
    let _ = menu.append(&remove_menu);
    let _ = menu.append(&PredefinedMenuItem::separator());

    let open_stations = MenuItem::with_id(OPEN_STATIONS, "Open Stations File…", true, None);
    let _ = menu.append(&open_stations);
    let reload = MenuItem::with_id(RELOAD_STATIONS, "Reload Stations", true, None);
    let _ = menu.append(&reload);

    let _ = menu.append(&PredefinedMenuItem::separator());
    let quit = MenuItem::with_id(QUIT, "Quit Radio", true, None);
    let _ = menu.append(&quit);

    Built {
        menu,
        handles: Handles {
            header,
            track,
            toggle,
            volume,
            stations: station_items,
        },
    }
}

pub fn header_text(player: &Player) -> String {
    match &player.current {
        Some(station) => format!("{} — {}", station.title, player.state.description()),
        None => player.state.description(),
    }
}

pub fn track_text(player: &Player) -> String {
    player
        .title
        .as_deref()
        .map(title::row)
        .unwrap_or_default()
}

pub fn toggle_text(player: &Player) -> &'static str {
    if player.is_active() {
        "Pause"
    } else {
        "Play"
    }
}

pub fn volume_text(volume: f64) -> String {
    format!("Volume: {}%", (volume * 100.0).round() as i64)
}
