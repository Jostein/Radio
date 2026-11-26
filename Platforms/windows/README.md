# Radio for Windows

A WinUI 3 app: the same radio player as the macOS build, with Windows' own tray,
its own Fluent panel and its own media stack. Nothing here is drawn by hand —
the panel is a `Window` with a Mica backdrop, the controls are stock WinUI, and
playback is `Windows.Media.Playback.MediaPlayer`, so the system theme, the accent
colour, the media flyout and the keyboard's media keys all work by themselves.

## Build

Needs the .NET 8 SDK on Windows; there is nothing else to install.

```
dotnet build -c Release
```

The result is `bin\Release\net8.0-windows10.0.19041.0\win-x64\Radio.exe`, with the
Windows App SDK beside it (the project is self-contained) so the folder runs as
it is. `WindowsPackageType=None` means no MSIX and no install step: start the exe
and the tray icon appears.

## How it works

| Piece | File | Notes |
| --- | --- | --- |
| Tray icon | `Tray.cs` | `Shell_NotifyIcon` through a hidden window, plus the right-click menu. Re-adds the icon when Explorer restarts. It is registered by window and id, not by a GUID: to the shell a GUID is a different icon, with none of what the user chose about the old one. |
| Panel | `FlyoutWindow.cs` | Borderless always-on-top window, positioned above the tray and closed when it loses focus. |
| Playback | `Player.cs` | `MediaPlayer` with `CommandManager` enabled, which is what puts the station in the Windows 11 media flyout. Media Foundation decodes MP3, AAC, Ogg and HLS, so **no ffmpeg is needed or shipped**. |
| Playlists | `Playlist.cs` | `.pls` and `.m3u` are resolved to their first entry; `.m3u8` goes to Media Foundation, which picks the best rendition. |
| Stations | `Stations.cs` | `%APPDATA%\Radio\stations.json`, the same file and format the macOS app and the Rust port use. |
| Add Station | `AddStationDialog.cs` | Its own small window: a 340-point panel cannot hold a `ContentDialog`. |

`RADIO_FLYOUT_DEMO=1` opens the panel at startup, pinned, for looking at it
without a hand on the tray — the same hook the Rust port uses on the build
worker.

## Verified, and not

Checked in the Parallels Windows 11 VM (26100, 200% scaling):

- builds from clean with the .NET 8 SDK alone: 0 warnings, 0 errors;
- the tray icon registers and survives an Explorer restart, still next to the
  clock where the user had put it;
- the panel draws as a Fluent card: station row, volume slider, accent
  play button and Add Station, correct size at 200% scaling;
- on first run only, it says where the tray switch is (a notification off the
  icon; clicking it opens Taskbar settings), because a new icon always starts
  in the overflow and only the user may promote it — verified in the VM;
- it drops from the tray icon (`Shell_NotifyIconGetRect`, so the icon is found
  wherever the tray put it) and falls back to the work area's bottom-right corner
  when the tray will not say;
- clicking a station starts the FIP Reggae HLS stream in `MediaPlayer`, the
  button turns into Pause, the row shows the playing marker and the status reads
  `Live`; the taskbar shows the media controls.

Not yet exercised: the tray icon being clicked by hand while the panel is up, the
Add Station window, the system media flyout's own buttons, the media keys, sound
coming out of a speaker (the VM's audio was not listened to), and any look at the
panel in light/dark/accent variants.
