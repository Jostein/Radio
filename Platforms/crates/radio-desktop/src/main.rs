#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
mod app;
mod audio;
/// The Add Station dialog, used by both front ends.
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod dialog;
#[cfg(target_os = "windows")]
mod find;
#[cfg(target_os = "windows")]
mod flyout;
mod icon;
mod media;
#[cfg(target_os = "linux")]
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
