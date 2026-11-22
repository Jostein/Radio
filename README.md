# Radio

A tiny internet radio player that lives in the menu bar. Add a stream URL, give
it a title, and tune in — no window, no Dock icon, no browser tab.

macOS is the original, Swift and AppKit build. Linux and Windows run the same app
from one Rust codebase, with the same stations file and the same menu. iPhone and
iPad use a native SwiftUI build in [`Mobile/iOS`](Mobile/iOS/README.md); Android
phones, tablets and TVs use a native Kotlin/Compose build in
[`Mobile/Android`](Mobile/Android/README.md). Light by design: 480 KB on disk,
15 MB of footprint when idle on this Mac — and about 2 MB of that is the app
rather than macOS.

## Download

Prebuilt desktop builds are on the [releases page](https://github.com/Jostein/Radio/releases/latest):

| Platform | Asset | Notes |
|---|---|---|
| **macOS 14+** | `Radio-1.3-macos.zip` | Universal Apple silicon and Intel app; ad-hoc signed, not notarized |
| **Windows 10/11 x86_64** | `Radio-1.3-windows-x86_64.zip` | Unsigned; `ffmpeg` must be on `PATH` |
| **Linux x86_64** | `radio-git-r62.97e4a9e-1-x86_64.pkg.tar.zst` | Arch/Omarchy package; other distros build from source |

The builds are independent of any store or AUR account. macOS and Windows are
not code-signed by a commercial certificate, so the OS may warn on first launch;
the source and build files are right here to check.

## What it does

- **Menu bar or tray only.** A waveform while playing, circling arrows while
  reconnecting, nothing in the Dock.
- **Stations with titles.** Added in a dialog that suggests the station's own
  name, removed from a submenu, remembered between launches.
- **Find stations by name.** A small panel searches the public Radio Browser
  directory and plays what you pick. The query runs only while the panel is open;
  nothing is bundled, preloaded or cached, so an idle app is unaffected.
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

macOS is the build in daily use. Linux has run in a container and a VM; Windows
has now run on an AppVeyor worker over RDP. Neither has met a normal laptop's own
desktop yet — [Platforms](#platforms) says exactly what has run.

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
[`Platforms/README.md`](Platforms/README.md). Arch and Omarchy can install the
package in `Platforms/packaging/arch` with `makepkg -si`; that README also has
the autostart and tray steps. GNOME, KDE/Plasma, Enlightenment and Omarchy host
notes live in [`Platforms/packaging/`](Platforms/packaging/). IRIX is not a
supported target: it lacks the modern Rust, GTK 3, ALSA, D-Bus and ffmpeg
runtime this app needs, and would require a separate legacy port.

## Platforms

| | Status |
|---|---|
| **macOS** | The original, in daily use. |
| **Linux** | Run end to end in a container and a VM. A laptop's own sound card is still untried. |
| **Windows** | Started on an AppVeyor worker over RDP: the tray menu and the FIP Reggae reconnect state were observed. Sound and media controls are still untested. |
| **iOS** | Native SwiftUI app in `Mobile/iOS`; simulator build and launch are verified in CI. No App Store archive yet. |
| **Android** | Native Kotlin/Compose app in `Mobile/Android`; the same APK has a Leanback launcher and D-pad focus for Android TV. Debug APK builds locally and in CI. Not yet launched on a device, TV or emulator. |

That last column is not modesty, it is the state of the evidence.
[`Platforms/README.md`](Platforms/README.md) records exactly what has run where.

## Details

Everything below is depth: how the streams are handled, the numbers, the files
the app writes, and where its limits are.

### Reconnect and the watchdog

A dropped or stalled stream is rebuilt with exponential backoff — 1s, 2s, 4s,
8s, 16s, 30s, six attempts — and the menu says `Reconnecting (2 of 6)…`. A
five-second watchdog catches a stream that has gone silent without reporting an
error, which is the failure that looks like "it just stopped": it only counts a
clock that is actually moving, because the player can say it is playing while
nothing is decoded. A stream the server closes ends the same way — the item
pauses by itself — so that is rebuilt at once, a second later, instead of
waiting to be judged silent. Pausing hangs up on the stream rather than holding
a buffer: the sound stops at the click, and resuming rejoins the live broadcast.

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

- **macOS:** a 480 KB binary, 15 MB footprint idle, of which 13 MB is an empty
  AppKit status item — the app is about 2 MB over its framework. Playing adds
  AVFoundation's buffer: about 22–23 MB footprint on an HLS stream with a
  six-second buffer, about 0.2% of a core. Progressive Icecast streams keep the default
  buffer: with a short forward buffer the player stops reading once it is full,
  the server closes the idle connection, and the stream restarts every few
  seconds. Getting 2 MB more by also telling AVPlayer not to wait for that
  buffer was tried and reverted — the app then reported playing while the stream
  sat silently at zero until the user paused and played again.
- **Linux:** 32.6 MB idle and 34.9 MB playing, 0.00% of a core when idle and
  about 0.65% playing, with ffmpeg alongside at 0.75%. Pausing releases the sound
  device rather than holding it.

Opening any dialog costs about 10 MB once: AppKit materialises its window and text
stack the first time a window is shown, and macOS keeps it. The add dialog has
always done this. Search adds roughly 2 MB while its panel is open, and releases
its results, request and session on close.

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
Sources/Radio/StationDirectory.swift  Radio Browser search, one request per query
Sources/Radio/StationFinder.swift  find-and-play panel, alive only while open
docs/index.html                   landing page
Platforms/                        Rust port for Linux and Windows
                                  (see Platforms/README.md)
Mobile/iOS/                       native SwiftUI iPhone/iPad app
                                  (see Mobile/iOS/README.md)
Mobile/Android/                   native Kotlin/Compose Android app
                                  (see Mobile/Android/README.md)
```

### Notes and limits

- Only the first entry of a playlist is used; there is no multi-entry selector.
- Search asks radio-browser.info by station name and needs a network connection.
  The app sends the typed words and nothing else; results are not stored.
- Search exists on macOS only. The Rust port, iOS app and Android app still have
  add-by-URL alone.
- One stream plays at a time.
- Android has no `.m3u`/`.pls` resolver or the desktop/iOS reconnect backoff yet;
  direct HLS and progressive streams use Media3's retry handling. The TV build
  is the same APK with a Leanback entry and remote-focus states, not a separate
  leanback layout.
- The macOS app is ad-hoc signed, not notarized and not sandboxed. The Windows
  build is unsigned. Both are usable from the releases page, but expect an
  operating-system warning on first launch.
- On Linux and Windows, stations are shown with a 60-character budget rather than
  a width measured to fit, and volume steps instead of a slider, because a tray
  menu has nowhere to put one.
- Windows has run only on an AppVeyor worker over RDP, where the tray menu and
  the FIP Reggae reconnect state were observed; its sound and media controls are
  still untested. Linux has not yet met a laptop's own sound card.
- `NSAllowsArbitraryLoadsForMedia` is enabled so plain-`http` radio streams work;
  the exception applies to media loads only.

## License

Released into the public domain under [CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/)
(SPDX: `CC0-1.0`) — see [LICENSE](LICENSE). No attribution required; do whatever
you like with it, including commercial use.
