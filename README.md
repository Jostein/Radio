# Radio

A tiny internet radio player that lives in the macOS menu bar. Add stream URLs,
give them titles, and tune in from the menu bar — no window, no Dock icon, no
browser tab.

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
  stations with a known JSON "now playing" feed (SomaFM, Radio Paradise, the FIP
  webradios) are read from that instead, which is cheaper and fresher. Anything
  else simply shows no track.
  Clicking the track line opens a YouTube search for it in your default browser.
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
Sources/Radio/PlaylistResolver.swift  .m3u/.pls fetching and parsing
Sources/Radio/StationStore.swift  JSON-backed station list
Sources/Radio/Station.swift       station model + URL validation
Sources/Radio/AddStationForm.swift  title/URL dialog with inline validation
```

## Notes and limits

- Playlists are detected by file extension, so a playlist served from an
  extensionless URL is not resolved.
- Only the first entry of a playlist is used; there is no multi-entry selector.
- One stream plays at a time.
- The app is ad-hoc signed for local use. It is not notarized and not sandboxed,
  so it is not ready for distribution as-is.
- `NSAllowsArbitraryLoadsForMedia` is enabled so plain-`http` radio streams work;
  the exception applies to media loads only.

## License

Released into the public domain under [CC0 1.0 Universal](https://creativecommons.org/publicdomain/zero/1.0/)
(SPDX: `CC0-1.0`) — see [LICENSE](LICENSE). No attribution required; do whatever
you like with it, including commercial use.
