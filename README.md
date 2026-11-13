# Radio

A tiny internet radio player that lives in the menu bar. Add stream URLs, give
them titles, and tune in from the menu bar or system tray — no window, no Dock
icon, no browser tab.

It runs on three desktops: **macOS, Linux and Windows**. macOS is the original,
Swift and AppKit build; Linux and Windows share one Rust codebase with the same
stations file and the same menu. All three are part of the app — [Platforms](#platforms)
says what each one has been shown to do.

## Features

- **Menu bar or tray only.** The status item shows a waveform while playing and
  circling arrows while reconnecting, on all three platforms.
- **Stations with titles.** Add a station with a title + URL, remove it from a
  submenu. The list is persisted between launches.
- **HLS and direct streams.** `.m3u8` (HLS) goes straight to `AVPlayer`; plain
  Icecast/MP3/AAC stream URLs play as-is.
- **`.m3u` and `.pls` playlists.** Many stations hand out playlist files instead
  of a stream address; these are fetched and resolved to the first stream entry,
  including relative references and `File1=` entries.
- **Reconnects on its own.** A dropped or stalled stream is rebuilt with
  exponential backoff (1s, 2s, 4s, 8s, 16s, 30s — six attempts), with the menu
  showing `Reconnecting (2 of 6)…`. A 5-second watchdog catches silent stalls,
  not just reported errors.
- **Sleep/wake aware.** Waking the Mac rejoins the live stream instead of
  trusting a connection that froze during sleep; waking while paused does
  nothing.
- **Now playing.** The current track appears under the station name while the
  station and connection state stay visible. Icecast/Shoutcast streams are read
  from ICY metadata (polled every 30s, with quick retries when a server is busy);
  HLS streams are read from timed metadata (ID3) when the stream carries it; and
  stations that publish a JSON "now playing" document are read from that instead,
  which is cheaper and fresher. Those are found automatically: Icecast servers
  answer on `/status-json.xsl` and Shoutcast servers on `/stats?json=1`, and a
  feed is used only when it names a title for the playing mount and is small
  enough to poll more cheaply than the audio block. A few broadcasters that
  block those endpoints (SomaFM, Radio Paradise, the FIP webradios) have their
  own feeds built in. Anything else simply shows no track.
  Clicking the track line opens a YouTube search for it in your default browser.
  The station and track also appear in the system's own media controls — Now
  Playing on macOS, MPRIS on Linux, SMTC on Windows — so Control Center, the
  keyboard media keys and AirPods or headphone buttons pause and resume.
- **Volume** slider in the menu, remembered between launches (defaults to 100%).

## Requirements

- macOS 14 or later for the Swift build. Xcode **not** required — Command Line
  Tools with Swift 6 / SwiftPM are enough.
- Linux: Rust 1.75+, ffmpeg, GTK 3, libayatana-appindicator, and `xdotool` on
  Arch for `libxdo`.
- Windows: Rust 1.75+, ffmpeg, and the MSVC build tools; `winget` installs the
  first two in two lines.

## Platforms

All three read the same `stations.json` format, seeded with FIP Reggae, and
behave the same way: menu bar or tray only, ICY and feed titles, reconnect with
backoff, media keys through the system's own controls.

### macOS

Swift and AppKit, macOS 14 or later — the build the app started as, and the one
it is used on every day. Menu bar item, Now Playing and media keys, sleep/wake,
playlists, volume.

```sh
make app    # builds build/Radio.app
make run    # builds and launches it
make clean  # removes .build and build
```

Then click the radio-waves icon in the menu bar. **Add Station…** takes a title
and any of: a direct stream URL, an `.m3u8`, an `.m3u` or an `.pls` URL.

The build pins SwiftPM's `--build-system native` flag, because the default build
system fails on Command Line Tools–only installs (it reports a property-list
parse error before compiling).

### Linux

One Rust binary per distro: tray icon, menu, an add-station dialog, playback,
MPRIS.

```sh
# Debian / Ubuntu
sudo apt install build-essential libgtk-3-dev libayatana-appindicator3-dev \
  libasound2-dev libxdo-dev pkg-config ffmpeg

cd Platforms
cargo run -p radio --release
```

Seen end to end on sway with waybar: the icon renders, the menu opens, a station
plays with its live title, pause, volume and quit work, and MPRIS reports each
change. In a Debian VM with PipeWire it registers as a client and plays audibly —
three seconds recorded from the sink monitor measured −14.4 dB mean, −0.4 dB peak.

While a station plays it sits at about 35 MB resident and under 1% of a core,
with ffmpeg alongside it at another 0.7%. Paused, it measures 0.00% of a core and
has already handed the sound device back.

Not yet seen: a laptop's own sound card (the VM's is virtual), the icon inside
GNOME's AppIndicator extension or Plasma's tray, edits to `stations.json` while
it runs, reconnect after a dropped stream, sleep/wake.

### Windows

The same Rust source, so it is the same `cargo run`. Rust and ffmpeg come from
winget; the MSVC build tools are the one long install, and rustup fetches them
when it asks.

```powershell
winget install Rustlang.Rustup
winget install Gyan.FFmpeg
cd Platforms
cargo run -p radio --release
```

It compiles, links and passes the core tests on a real Windows runner on every
push (`.github/workflows/windows.yml`), and the hidden window SMTC binds to is
created before the app starts. The tray, the media flyout and the media keys have
**never been started** — a runner has no interactive session for a notification
area to live in.

### What differs, on purpose

Adding a station is each platform's own dialog — an AppKit alert, a GTK dialog, a
small Win32 window — with the same two fields, the same validation and the same
title suggestion. Volume is a slider on macOS and stepped items in the tray
builds, because a tray menu has nowhere to put a slider. And the track line is a
60-character budget rather than a width measured to fit.

The port's own [`README`](Platforms/README.md) has the per-distro packages, the
parity matrix and the list of expected failure points; if one of those builds
misbehaves, that is a gap in evidence rather than a mystery.

## Keyboard

A status-bar app has no visible menu bar, but the app still installs a minimal
hidden main menu so standard editing shortcuts work inside the add-station
dialog: ⌘C, ⌘V, ⌘X, ⌘A, ⌘Z, plus ⌘Q to quit.

## Where data lives

- Stations: `~/Library/Application Support/Radio/stations.json`
- Volume: `UserDefaults` for `local.radio.menubar` (key `volume`)

The Rust build uses the same file names in the platform's config directory:
`~/.config/Radio/` on Linux, `%APPDATA%\Radio\` on Windows. Same JSON array, so
the station list travels between machines.

Deleting the JSON file restores the single starter station (FIP Reggae).

## Project layout

```
Package.swift                     SwiftPM manifest (macOS 14+, AppKit)
Makefile                          build / run / clean targets
Resources/Info.plist              LSUIElement, media ATS exception, icon name
Resources/AppIcon.icns            app icon (used by alerts and Finder)
Sources/Radio/main.swift          NSApplication entry, .accessory policy
Sources/Radio/AppDelegate.swift   status item, menu, volume slider, actions
Sources/Radio/RadioPlayer.swift   AVPlayer wrapper: state machine, backoff, watchdog, wake
Sources/Radio/NowPlayingMonitor.swift  polling engine for ICY and feeds, plus HLS metadata
Sources/Radio/NowPlayingFeeds.swift    known broadcaster feeds and generic discovery
Sources/Radio/ICYTitleReader.swift  one ICY metadata block per read
Sources/Radio/TrackUpdate.swift   title / no title / unavailable result
Sources/Radio/SystemNowPlaying.swift  Control Center and media keys
Sources/Radio/TrackLabel.swift    width-fitted now-playing row
Sources/Radio/PlaylistResolver.swift  .m3u/.pls fetching and parsing
Sources/Radio/StreamMetadata.swift  suggested station name for a stream URL
Sources/Radio/YouTubeSearch.swift  track search in the default browser
Sources/Radio/StationStore.swift  JSON-backed station list
Sources/Radio/Station.swift       station model + URL validation
Sources/Radio/AddStationForm.swift  title/URL dialog with inline validation
docs/index.html                   landing page
Platforms/                        Rust port for Linux and Windows
                                  (see Platforms/README.md)
```

## Notes and limits

- Playlists are detected by file extension, so a playlist served from an
  extensionless URL is not resolved.
- Only the first entry of a playlist is used; there is no multi-entry selector.
- One stream plays at a time.
- The app is ad-hoc signed for local use. It is not notarized and not sandboxed,
  so it is not ready for distribution as-is.
- Windows has never been started, and Linux has not yet met a laptop's own
  sound card. Both are part of the app and both build from source;
  [Platforms](#platforms) lists exactly which parts have run where.
- `NSAllowsArbitraryLoadsForMedia` is enabled so plain-`http` radio streams work;
  the exception applies to media loads only.

## License

Released into the public domain under [CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/)
(SPDX: `CC0-1.0`) — see [LICENSE](LICENSE). No attribution required; do whatever
you like with it, including commercial use.
