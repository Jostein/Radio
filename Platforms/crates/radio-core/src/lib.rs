//! Platform-independent parts of the Radio player.
//!
//! Everything here is synchronous and free of windowing or audio libraries, so
//! it can be unit tested on any platform and reused by the tray app.

pub mod feeds;
pub mod icy;
pub mod metadata;
pub mod monitor;
pub mod playlist;
pub mod search;
pub mod station;
pub mod store;
pub mod title;

pub use icy::TrackUpdate;
pub use metadata::suggested_title;
pub use monitor::{Monitor, Update};
pub use station::Station;
pub use store::Store;
