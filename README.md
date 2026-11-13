# Radio

A tiny internet radio player that lives in the macOS menu bar. Add stream URLs,
give them titles, and tune in from the menu bar — no window, no Dock icon, no
browser tab.

There is also a Rust port for **Linux and Windows** in [`Platforms/`](Platforms/README.md).
It builds and plays, but it has not been run on a real desktop yet — read
[that status](#linux-and-windows) before relying on it.

## Features

- **Menu bar only.** The status item shows a waveform while playing and circling
  arrows while reconnecting.
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
  The station and track also appear in macOS's Now Playing panel, so Control
  Center, the keyboard media keys and AirPods play/pause all work.
- **Volume** slider in the menu, remembered between launches (defaults to 100%).

## Requirements

- macOS 14 or later.
- Xcode **not** required — Command Line Tools with Swift 6 / SwiftPM are enough.

## Build and run

```sh
make app    # builds build/Radio.app
make run    # builds and launches it
make clean  # removes .build and build
```

Then click the radio-waves icon in the menu bar. **Add Station…** takes a title
and any of: a direct stream URL, an `.m3u8`, an `.m3u` or a `.pls` URL.

The build pins SwiftPM's `--build-system native` flag, because the default build
system fails on Command Line Tools–only installs (it reports a property-list
parse error before compiling).

## Linux and Windows

A Rust port of the same app lives in [`Platforms/`](Platforms/README.md): one
binary per system, the same `stations.json`, the same behaviour — tray only,
ICY and feed titles, reconnect with backoff, media keys through MPRIS on Linux
and SMTC on Windows.

**It is early, and it has only run inside containers and a VM.** What that means:

- Both binaries build from source on every platform, with no warnings. The
  Windows build also compiles and passes its unit tests on a real Windows runner
  on every push, through `.github/workflows/windows.yml`.
- On Linux it has been run three times. Headless (Xvfb, a private session bus,
  ALSA `null` output) it decodes a live stream to 44.1 kHz PCM at real time and
  publishes MPRIS controls that `playerctl` drives. On a real compositor (sway
  with a waybar tray host) the icon renders, the menu opens, choosing a station
  plays it, the header shows the ICY title, and pause, volume and quit work. In a
  Debian VM with PipeWire it registers as a client and plays audibly: three
  seconds recorded from the sink monitor measured −14.4 dB mean, −0.4 dB peak.
- Still unproven on Linux: a laptop's own sound card (PipeWire was proven on a
  VM's virtual one), the icon inside GNOME's AppIndicator extension or Plasma's
  tray, editing `stations.json` while it runs, reconnect after a dropped stream,
  and sleep/wake.
- The Windows build compiles and links but **has never been started**. The media
  flyout, the media keys, and the hidden window SMTC is bound to are unproven.

If it does not work on your machine, that is expected and fixable. The port is a
small, plain Rust codebase whose README names the parts that were never
exercised, which makes it a good job for an AI coding agent: open the
repository, run it, paste what you see, and ask for that one thing to be fixed.

On either system, from the repository root:

```sh
cd Platforms
cargo run -p radio --release
```

Windows needs Rust and ffmpeg, and nothing else: both come from winget, as
`Rustlang.Rustup` and `Gyan.FFmpeg`, with the MSVC build tools that rustup
offers to install. Linux needs those plus GTK 3,
libayatana-appindicator and, on Arch, `xdotool` — the tray library links
`libxdo`. The port's own [`README`](Platforms/README.md) has the per-distro
packages, the parity matrix, and the list of expected failure points.

## Keyboard

A status-bar app has no visible menu bar, but the app still installs a minimal
hidden main menu so standard editing shortcuts work inside the add-station
dialog: ⌘C, ⌘V, ⌘X, ⌘A, ⌘Z, plus ⌘Q to quit.

## Where data lives

- Stations: `~/Library/Application Support/Radio/stations.json`
- Volume: `UserDefaults` for `local.radio.menubar` (key `volume`)

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
- The Linux and Windows port is unproven on real desktops. Treat it as a
  starting point that an agent or a person can finish, not as a release.
- `NSAllowsArbitraryLoadsForMedia` is enabled so plain-`http` radio streams work;
  the exception applies to media loads only.

## License

Released into the public domain under [CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/)
(SPDX: `CC0-1.0`) — see [LICENSE](LICENSE). No attribution required; do whatever
you like with it, including commercial use.
