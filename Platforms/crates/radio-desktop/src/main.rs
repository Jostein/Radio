mod app;
mod audio;
mod dialog;
mod icon;
mod media;
mod menu;
mod platform;
mod player;

#[cfg(target_os = "linux")]
mod mpris;

#[cfg(target_os = "windows")]
mod smtc;

fn main() {
    if let Err(error) = platform::run() {
        eprintln!("radio: {error}");
        std::process::exit(1);
    }
}
