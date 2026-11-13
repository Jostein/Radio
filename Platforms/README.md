# Radio for Linux and Windows

The Linux and Windows side of the app: one shared Rust codebase, the same
behaviour as the macOS build in the repository root. A tray icon instead of a
menu bar item, MPRIS or SMTC instead of Now Playing, `~/.config/Radio` or
`%APPDATA%\Radio` instead of `~/Library/Application Support/Radio`.

The macOS build is documented in the [root README](../README.md). All three are
meant to behave alike; this file records where the evidence for these two
currently stops.

> **Status: runs on Linux, builds and tests itself on Windows.** Verified with
> Rust 1.100:
>
> - `cargo build` / `cargo build --release` — clean on macOS.
> - `cargo test -p radio-core` — 17 unit tests pass on macOS and on Linux.
> - Linux (aarch64, GTK 3, in a container): builds and links a 5.0 MB binary
>   against `libgtk-3`, `libasound` and `libxdo`.
> - Windows: `cargo build --release --target x86_64-pc-windows-gnu` links a
>   5.5 MB `radio.exe` from macOS (needs `mingw-w64`), SMTC included. The Win32
>   message pump and the hidden window type-check against the MSVC target too.
>   CI builds it with MSVC and runs the core tests on every change to
>   `Platforms` — 2.9 MB, 17/17 (`.github/workflows/windows.yml`).
>   **Nothing has been run on Windows yet.**
> - Linux, headless (Xvfb, a private session bus, ALSA `null` output): the binary
>   starts, loads libayatana-appindicator, decodes a live MP3 stream to 44.1 kHz
>   PCM through cpal/ALSA at real time, and shows ICY track titles within about a
>   second of tuning in.
>
> - Linux media controls: MPRIS is published on the session bus and checked with
>   `playerctl` — `Playing`/`Paused` reporting, the live track title as
>   `xesam:title` with the station as `xesam:artist`, play / pause / play-pause /
>   stop, the writable `Volume` property, and `PropertiesChanged` signals for
>   every one of them.
> - Linux audio, in a Debian 13 VM (OrbStack: systemd, PipeWire 1.4.2, a virtio
>   sound card): the app registers as a PipeWire client (`PipeWire ALSA
>   [radio-bin]`), a station started from the tray menu plays through it — three
>   seconds recorded from the sink monitor measured `mean_volume: -14.4 dB`,
>   `max_volume: -0.4 dB` — and pause returns MPRIS to `Paused`. The card is
>   virtual; a laptop's own audio hardware is still unproven.
> - Windows media controls: SMTC compiles and links, and the hidden window it is
>   bound to is created before the app starts. Needs a real Windows to prove the
>   flyout, the media keys and the metadata.
>
> **Linux tray and menu: observed.** In the same container, with sway on its
> headless backend and waybar's `tray` module as the host, the 32×32 icon renders
> in the panel and the menu opens with every item. Driving that menu the way a
> click does — DBusMenu `Event`, which reaches the same handler — a station
> starts playing, the header becomes `FIP Reggae — Live` (feed poller and ICY
> titles), Play becomes Pause, pause returns to `Paused`, Quieter steps the
> volume 1.00 → 0.90 while Louder at 100% stays clamped, and Quit Radio exits
> with code 0. MPRIS reports the state changes.
>
> **Still not observed:** `stations.json` edits while the app runs, reconnect
> after a dropped stream, sleep/wake, resuming at the live edge after a pause,
> the icon inside GNOME's AppIndicator extension or Plasma's tray (only waybar
> has hosted it), a real pointer click on a real menu (activation went over
> D-Bus, not through a cursor), HiDPI, multi-monitor, and sound through a
> laptop's own audio hardware — PipeWire was proven on a VM's virtual card. And
> on Windows, the whole runtime: it builds, links and passes its tests, but no
> tray, no flyout, no media key has ever run there.
>
> Two things running it on Linux already fixed: the output is now configured at
> the device's own default rate (asking for the maximum rate is rejected outright
> by some backends) and the stall watchdog counts arriving samples instead of the
> buffer fill level (a device that drains eagerly shows an empty buffer while
> playing perfectly).

## If it does not work

If the icon never appears, the menu opens somewhere odd, or a station will not
start, that is the port being young, not you. It has run in a container and one
VM, never on a real desktop, so your machine is allowed to be the first one that
disagrees.

Start it from the repository root:

```sh
cd Platforms
cargo run -p radio --release
```

Then collect three small things, which is usually enough to find the cause:

```sh
# Linux
echo "$XDG_CURRENT_DESKTOP"                 # GNOME, KDE, Hyprland, sway, ...
command -v ffmpeg && ffmpeg -version | head -1
playerctl -p radio status                   # only while the app is running
```

```powershell
# Windows
echo $env:APPDATA
ffmpeg -version
```

Paste that with one sentence about what you saw. The quickest route to a fix is
an AI coding agent: open the repository in one, say what happened, and ask for
that one thing — the known gaps at the bottom name the parts most likely to be
wrong, so an item from that list is enough to start. An issue works just as
well.

## What is shared and what is not

```
Platforms/
  Cargo.toml                     workspace
  crates/radio-core/             no UI, no audio, unit tested
    station.rs                   station model, same JSON keys as the macOS app
    store.rs                     stations.json + settings.json (volume)
    playlist.rs                  .m3u /.pls resolution
    feeds.rs                     SomaFM / Radio Paradise / FIP + Icecast and
                                 Shoutcast discovery, 16 KB cap
    icy.rs                       one ICY metadata block per read
    monitor.rs                   the single polling engine: feed first, ICY
                                 fallback, retries, generation guard
    title.rs                     track row text and truncation
    search.rs                    YouTube search URL
  crates/radio-desktop/          the tray app
    app.rs                       store + player + menu wiring, file watching
    audio.rs                     cpal output, ffmpeg PCM pump
    player.rs                    state machine, reconnect backoff, watchdog
    menu.rs                      tray menu
    media.rs                     media front end: one vocabulary, per-OS backend
    mpris.rs                     Linux: the desktop's media widget (D-Bus)
    smtc.rs                      Windows: SMTC, media keys and the flyout
    platform.rs                  GTK loop (Linux), message pump + hidden window
                                 (Windows)
```

The macOS app in the repository root is untouched and stays the reference
implementation.

## Build and run

Needs Rust 1.75 or newer (`rustup`), plus an audio stack and `ffmpeg` on `PATH`.

**Arch / Omarchy**

```sh
sudo pacman -S rust gtk3 libayatana-appindicator alsa-lib ffmpeg pkgconf xdotool
cd Platforms
cargo run -p radio
```

`xdotool` is there for `libxdo`, which the tray library links against; without it
the link step fails with `cannot find -lxdo`.

**Debian / Ubuntu** (what was actually built and run here)

```sh
sudo apt install build-essential libgtk-3-dev libayatana-appindicator3-dev \
  libasound2-dev libxdo-dev pkg-config ffmpeg
```

`libappindicator-gtk3` works too if `libayatana-appindicator` is already taken by
something else. On Hyprland the icon appears in whatever hosts a tray (waybar's
`tray` module, for example); without a tray host there is nothing to click.

**Windows** (PowerShell)

```powershell
winget install Rustlang.Rustup     # offers to install the MSVC build tools
winget install Gyan.FFmpeg         # skip if you already have an ffmpeg on PATH
cd Platforms
cargo run -p radio --release
```

The MSVC build tools are the only long install, and rustup fetches them for you
when it asks. Nothing else is needed: the tray library, the audio backend and
the media keys are all Windows system libraries.

CI does that build and the core tests on `windows-latest`
(`.github/workflows/windows.yml`). It cannot test the tray: a runner has no
interactive session for a notification area to live in.

The icon appears in the notification area. Windows hides new tray icons behind
the overflow chevron until you drag it out.

## Behaviour checklist for the first run

In the order worth testing, because each later item depends on the earlier ones.
On Linux, 1–3 and 7 have been exercised — sway/waybar for the tray, a PipeWire VM
for playback and sound — and 4 only as far as pause: resume and rejoining the
live edge have not. 5, 6 and everything on Windows are untested:

1. The tray icon appears and the menu opens.
2. A station plays (the default is FIP Reggae, which is an `.m3u8` and therefore
   exercises the ffmpeg path).
3. The track row fills in. FIP uses a built-in feed, so this proves the feed
   poller. An NRK mount proves ICY; a random Icecast station proves discovery.
4. Pause keeps the last track; Play resumes at the live edge.
5. Killing the network (or the station) shows `Reconnecting (n of 6)…` and
   recovers.
6. Editing `stations.json` while the app runs updates the menu within ~2 s.
7. The desktop's own media UI shows the station and track, and its play/pause
   buttons drive the app: `playerctl -p radio status` on Linux, the media flyout
   on Windows.

Three things to watch for, because they are the parts most likely to need a fix:

- On Windows the tray is created just before the message pump starts. If menu
  clicks do nothing, the tray has to move inside the loop (create it on the
  first iteration) — the `muda` docs are explicit that a running event loop is
  required on that platform.
- A pause during a slow metadata read clears the track row when that read
  finishes, which can be up to 8 s later. The macOS app answers instantly
  because its reads are asynchronous.
- SMTC is bound to a top-level window, and this app has none, so `platform.rs`
  creates a hidden one before anything else. If the media flyout stays empty on
  Windows, that window is the first suspect: it may need to be visible (or at
  least a different style) for the system to accept it. The app keeps working
  without it — media controls simply report themselves as off.

## Parity with the macOS app

| Feature | macOS | Port |
|---|---|---|
| Menu bar / tray only | yes | yes |
| Stations by URL + title | dialog | JSON file, watched for edits |
| Volume | slider, persisted | Louder / Quieter items, persisted |
| ICY, feeds, discovery | yes | yes (same rules, same caps) |
| HLS | AVPlayer | ffmpeg |
| Reconnect with backoff | 1–30 s, 6 attempts | same |
| Stall watchdog | 5 s × 4 | 1 s × 4 on a stalled sample counter |
| Pause keeps track, resume rejoins live | yes | yes |
| Sleep/wake recovery | yes | not yet |
| Click track to search YouTube | yes | yes |
| Control Center / media keys | `MPNowPlayingInfoCenter` | MPRIS on Linux, SMTC on Windows |
| Add station from the app | dialog | edit the JSON file |
| Width-fitted title | measured | 60-character budget |

## Deliberate choices

- **Rust** for one small binary per platform with no runtime, matching the
  "stay lean" goal of the macOS app.
- **`tray-icon` + `muda`** for the tray and menu: native menus on Windows, GTK
  indicators on Linux. There is no cross-platform slider, hence Louder/Quieter.
- **`cpal` + `ffmpeg`** for audio instead of a Rust decoder: ffmpeg is the only
  practical way to cover MP3, AAC, Ogg *and* HLS with one code path. It costs a
  child process; a native decoder for plain streams is the obvious follow-up.
  If ffmpeg is missing the app says so instead of failing silently.
- **No window toolkit** beyond what the tray needs; the app loop is a 25 Hz
  poll, which is what makes the same code work under GTK and under the Win32
  message pump.

## Media controls

Besides the tray, the app appears in the operating system's own media UI: the
media widget or Quick Settings tile on Linux, the media flyout on Windows. Same
vocabulary in `media.rs`, one backend per OS. Where a backend cannot be created
the app says so and runs with the tray alone.

### Linux (MPRIS)

The app publishes `org.mpris.MediaPlayer2.radio` on the session
bus, so the desktop's own media widget drives it: Plasma's media widget, GNOME's
Quick Settings controls, Cinnamon's and Budgie's sound applets, XFCE's mpris
plugin, waybar's `mpris` module, and `playerctl` in any keybinding. Nothing is
configured per desktop; one implementation serves all of them.

What it reports:

- `PlaybackStatus` — `Playing`, `Paused`, or `Stopped` while connecting.
- `Metadata` — the current track as `xesam:title`, the station as `xesam:artist`.
  No length and no artwork: a live stream has neither.
- `Volume` — a real writable property, so the widget's slider works and the value
  is saved the same way the menu's Louder/Quieter items save it.

What it deliberately does not do: seek or skip, because a stream has no position
and no track list. `CanSeek`, `CanGoNext` and `CanGoPrevious` are false rather
than lying. `Stop` behaves like pause, since there is no end to stop at.

The tray menu and the media widget send the same actions through the same code,
so they cannot disagree; a change from either one is signalled to the other.

```sh
playerctl -p radio status          # Playing
playerctl -p radio metadata        # xesam:title / xesam:artist
playerctl -p radio play-pause
playerctl -p radio volume 0.4      # also persisted to settings.json
```

What it costs: `zbus` is the one heavyweight dependency in the port, about 2.5 MB
of code. It needs no extra process and no background polling — the service runs
on a thread inside the app and signals only when something changes. The release
profile strips symbols, which puts the whole Linux binary at 5.0 MB.

### Windows (SMTC)

On Windows the same state goes to `SystemMediaTransportControls`, which is what
the media flyout, the lock screen, the volume overlay and the keyboard's media
keys read. What that means in practice:

- The station and track show as title and artist, refreshed when the ICY title
  changes.
- Play, pause and stop from the flyout or the media keys drive the app, through
  the same actions the tray menu uses.
- Play is disabled until a station is loaded, matching the tray's disabled Play
  item.
- No volume: SMTC has no volume property. The tray's Volume submenu stays the
  only volume control on Windows, and `settings.json` is still the record.

SMTC is not a bus protocol like MPRIS; the object belongs to a top-level window
of the process. Since this app is tray-only, `platform.rs` creates a hidden
window purely to own it, before the app is built. The release binary is 5.5 MB.

## Data files

Same `stations.json` array as macOS — copy it over and the list comes with you.

- Linux: `~/.config/Radio/stations.json`, `~/.config/Radio/settings.json`
- Windows: `%APPDATA%\Radio\stations.json`, `%APPDATA%\Radio\settings.json`

When `stations.json` is missing, the app starts with FIP Reggae, exactly like
the macOS build.

## Known gaps, in the order I would fix them

1. Work through the checklist on Windows, where none of it has run: the tray,
   its menu, the media flyout and the media keys are the parts with no evidence
   behind them. On Linux the icon, the menu, pause, volume, quit, MPRIS and
   playback through PipeWire are done; editing `stations.json` while it runs (6)
   and reconnect (5) are not, and no laptop's own sound card has been tried.
2. Prove SMTC on a real Windows. It compiles and links, but the flyout, the
   hidden window and the media keys have never been exercised; the window style
   is the part I would expect to need adjusting.
3. A proper add-station dialog (GTK dialog on Linux, a small Win32 one on
   Windows); the file is a workable stopgap.
4. Sleep/wake handling: a laptop resume should rejoin the stream, as the macOS
   build does.
5. Native decoding for non-HLS streams, dropping ffmpeg for the common case.
