use crate::station::Station;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Station list and volume, stored as JSON next to each other.
///
/// `dirs::config_dir()` resolves to the same folder the macOS app uses
/// (`~/Library/Application Support/Radio`) on macOS, to
/// `~/.config/Radio` on Linux and `%APPDATA%\Radio` on Windows.
pub struct Store {
    pub stations: Vec<Station>,
    /// 0.0 ... 1.0
    pub volume: f64,
    directory: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    volume: f64,
}

impl Store {
    pub fn load() -> Store {
        let directory = config_directory();
        let mut store = Store {
            stations: Vec::new(),
            volume: 1.0,
            directory,
        };
        store.reload();
        store.load_settings();
        store
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn stations_file(&self) -> PathBuf {
        self.directory.join("stations.json")
    }

    fn settings_file(&self) -> PathBuf {
        self.directory.join("settings.json")
    }

    /// Reads the station list from disk, falling back to the starter station.
    /// Returns true when the loaded list differs from what is in memory.
    pub fn reload(&mut self) -> bool {
        let loaded = match std::fs::read(self.stations_file()) {
            Ok(data) => serde_json::from_slice::<Vec<Station>>(&data).unwrap_or_else(|_| starter()),
            Err(_) => starter(),
        };

        let changed = loaded.len() != self.stations.len()
            || loaded
                .iter()
                .zip(self.stations.iter())
                .any(|(a, b)| a.id != b.id || a.url_string != b.url_string || a.title != b.title);

        self.stations = loaded;
        changed
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(&self.directory);
        if let Ok(data) = serde_json::to_vec_pretty(&self.stations) {
            let _ = std::fs::write(self.stations_file(), data);
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.stations.len();
        self.stations.retain(|station| station.id != id);
        let removed = self.stations.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    pub fn station(&self, id: &str) -> Option<&Station> {
        self.stations.iter().find(|station| station.id == id)
    }

    pub fn set_volume(&mut self, volume: f64) {
        self.volume = volume.clamp(0.0, 1.0);
        let settings = Settings {
            volume: self.volume,
        };
        if let Ok(data) = serde_json::to_vec_pretty(&settings) {
            let _ = std::fs::create_dir_all(&self.directory);
            let _ = std::fs::write(self.settings_file(), data);
        }
    }

    fn load_settings(&mut self) {
        if let Ok(data) = std::fs::read(self.settings_file()) {
            if let Ok(settings) = serde_json::from_slice::<Settings>(&data) {
                self.volume = settings.volume.clamp(0.0, 1.0);
            }
        }
    }

    pub fn modified(&self) -> Option<SystemTime> {
        std::fs::metadata(self.stations_file())
            .and_then(|meta| meta.modified())
            .ok()
    }
}

fn config_directory() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Radio")
}

fn starter() -> Vec<Station> {
    vec![Station::new(
        "FIP Reggae",
        "https://stream.radiofrance.fr/fipreggae/fipreggae.m3u8",
    )]
}
