# Radio

A tiny internet radio player that lives in the menu bar. Add a stream URL, give
it a title, and tune in — no window, no Dock icon, no browser tab.

macOS is the original, Swift and AppKit build. Linux and Windows run the same app
from one Rust codebase, with the same stations file and the same menu. Light by
design: 370 KB on disk, 16 MB of footprint when idle on this Mac — and about 3 MB
of that is the app rather than macOS.

## What it does

- **Menu bar or tray only.** A waveform while playing, circling arrows while
  reconnecting, nothing in the Dock.
- **Stations with titles.** Added in a dialog that suggests the station's own
  name, removed from a submenu, remembered between launches.
- **Any stream.** `.m3u8` HLS, plain Icecast MP3 or AAC, and `.m3u` or `.pls`
  playlist links, resolved to the stream inside.
- **Keeps playing.** Dropped or stalled streams are rebuilt with backoff, and a
  watchdog catches silence that never reports an error.
- **Shows what is playing.** The track appears under the station name, read from
  the stream or from the station's own feed; click it to search YouTube.
- **Plays with your media keys.** Now Playing on macOS, MPRIS on Linux, SMTC on
  Windows: the system's own controls pause and resume the radio.
- **Remembers your volume**, at 100% out of the box.

## Quick start

macOS is the build that is tested. The Linux and Windows commands are real and
work on this project's own test bench, but neither has met a normal desktop yet —
[Platforms](#platforms) says exactly what has run.

```sh
# macOS 14+, Command Line Tools. The Xcode app is not needed.
git clone https://github.com/Jostein/Radio.git
cd Radio
make run
```

```sh
# Linux: Rust 1.75+, ffmpeg, GTK 3, libayatana-appindicator
sudo apt install build-essential libgtk-3-dev libayatana-appindicator3-dev \
  libasound2-dev libxdo-dev pkg-config ffmpeg
cd Platforms
cargo run -p radio --release
```

```powershell
# Windows, in PowerShell
winget install Rustlang.Rustup    # offers the MSVC build tools
winget install Gyan.FFmpeg
cd Platforms
cargo run -p radio --release
```

The per-distro packages and the Arch-specific `xdotool` note are in
[`Platforms/README.md`](Platforms/README.md).

## Platforms

| | Status |
|---|---|
| **macOS** | The original, in daily use. |
| **Linux** | Run end to end in a container and a VM. A laptop's own sound card is still untried. |
| **Windows** | Compiles, links and passes the core tests on a real Windows runner. Never started. |

That last column is not modesty, it is the state of the evidence.
[`Platforms/README.md`](Platforms/README.md) records exactly what has run where.

## Details

Everything below is depth: how the streams are handled, the numbers, the files
the app writes, and where its limits are.

### Reconnect and the watchdog

A dropped or stalled stream is rebuilt with exponential backoff — 1s, 2s, 4s,
8s, 16s, 30s, six attempts — and the menu says `Reconnecting (2 of 6)…`. A
five-second watchdog catches a stream that has gone silent without reporting an
error, which is the failure that looks like "it just stopped".

### How the track is found

Icecast and Shoutcast streams are read from ICY metadata, polled every 30s with
quick retries when a server is busy. HLS streams are read from timed metadata
(ID3) when they carry it. Stations that publish a JSON "now playing" document are
read from that instead, because it is cheaper and fresher: Icecast servers answer
on `/status-json.xsl`, Shoutcast on `/stats?json=1`, and a feed is used only when
it names a title for the playing mount and is small enough to poll more cheaply
than the audio block. A few broadcasters that block those endpoints — SomaFM,
Radio Paradise, the FIP webradios — have their own feeds built in. Anything else
simply shows no track.

### Playlists

`.m3u` and `.pls` links are fetched and resolved to their first stream entry,
including relative references and `File1=` entries. A playlist served from an
extensionless URL is not resolved, because detection is by extension.

### Sleep and wake

On macOS, waking rejoins the live stream instead of trusting a connection that
froze during sleep. Waking while paused does nothing. The Rust build has the same
backoff and watchdog; wake has not been tried there.

### Media keys

The station and track go to the system's own media UI, so Control Center, the
keyboard media keys and AirPods or headphone buttons work.

### Performance

Measured on this laptop and in a Debian VM:

- **macOS:** a 370 KB binary, 16 MB footprint idle, of which 13 MB is an empty
  AppKit status item — the app is about 3 MB over its framework. Streaming adds
  AVFoundation's buffer, which a live stream does not need in full: asking for six
  seconds instead of the default measured 16 MB → 13 MB for the same station, and
  starts sooner.
- **Linux:** 32.6 MB idle and 34.9 MB playing, 0.00% of a core when idle and
  about 0.65% playing, with ffmpeg alongside at 0.75%. Pausing releases the sound
  device rather than holding it.

### Where data lives

- Stations: `~/Library/Application Support/Radio/stations.json`
- Volume: `UserDefaults` for `local.radio.menubar` (key `volume`)

The Rust build uses the same file names in the platform's config directory:
`~/.config/Radio/` on Linux, `%APPDATA%\Radio\` on Windows. Same JSON array, so
the station list travels between machines. Deleting the JSON file restores the
single starter station (FIP Reggae).

### Keyboard

A status-bar app has no visible menu bar, but the app still installs a minimal
hidden main menu so standard editing shortcuts work inside the add-station
dialog: ⌘C, ⌘V, ⌘X, ⌘A, ⌘Z, plus ⌘Q to quit.

### Project layout

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

### Notes and limits

- Only the first entry of a playlist is used; there is no multi-entry selector.
- One stream plays at a time.
- The app is ad-hoc signed for local use. It is not notarized and not sandboxed,
  so it is not ready for distribution as-is.
- On Linux and Windows, stations are shown with a 60-character budget rather than
  a width measured to fit, and volume steps instead of a slider, because a tray
  menu has nowhere to put one.
- Windows has never been started, and Linux has not yet met a laptop's own sound
  card.
- `NSAllowsArbitraryLoadsForMedia` is enabled so plain-`http` radio streams work;
  the exception applies to media loads only.

## License

Released into the public domain under [CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/)
(SPDX: `CC0-1.0`) — see [LICENSE](LICENSE). No attribution required; do whatever
you like with it, including commercial use.
