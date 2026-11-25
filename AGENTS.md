# Radio

A macOS menu-bar radio app, with ports under `Platforms/` for Windows and Linux.
The macOS app in the repository root is the reference implementation.

## Testing the Windows tray app

The tray, its flyout and playback need an interactive Windows desktop. GitHub's
runners have none, so the interactive test runs on AppVeyor, whose free worker
can pause a build for Remote Desktop.

One command, from the repository root:

```
scripts/test-env.sh
```

It attaches to a paused worker when one is alive, otherwise waits for a build
already queued, starts one if there is none, and opens Windows App with the
host, user and password filled in (the password also lands on the clipboard).
Sound is set to play on this computer, so what the app plays comes out of the
Mac. It runs unattended: start it in the background and the session opens by
itself when the pause arrives.

What the build does before pausing (`appveyor.yml`, `on_finish`):

- installs ffmpeg beside `radio.exe` if it is missing,
- starts `radio.exe`, so the tray icon is already in the notification area,
- expires the pause by itself after `RADIO_RDP_MINUTES` (20 by default), so no
  worker is held after the session ends.

### Traps, all of them learned the hard way

- **One worker per account.** Every build that pauses holds the only slot, so a
  queue of paused builds costs an hour each. Cancel the ones you do not need:
  `DELETE /api/builds/{account}/{project}/{version}` — the cancel endpoints
  every other shape (`/builds/{id}/cancel`, `/buildsjobs/{id}`) return 404/405.
  Deleting a queued build also works: `DELETE /api/builds/{buildId}`.
- **AppVeyor runs `on_finish` with `Platforms/` as the working directory**, so a
  path there is relative to `Platforms`, not to the repository root. The launch
  step silently failed on this for several builds: `Start-Process` reported an
  error but the build carried on to the pause with an empty desktop.
- **The pause is not a prompt to type at.** Typing into the session over RDP is
  unreliable and lands in whichever window has focus on the client. Put
  everything that must happen in `on_finish`.
- **The Mac client keeps no window once its session dies**, and re-opening the
  `.rdp` then shows its device list forever. Quit Windows App and open it again;
  `test-env.sh` does this by itself when no session window appears.
- **`screencapture` cannot capture the Remote Desktop window** (`could not
  create image from window`), and the client's fullscreen mode steals a Space on
  this Mac. The `.rdp` file uses a window for both reasons.
- **The audio endpoint probe in `on_finish` always reports "no audio endpoints"**,
  because it runs before any RDP session exists, so it says nothing about
  playback. Audio in the session works when the client plays sound locally,
  which the `.rdp` file sets (`audiomode:i:0`).
- Playback is only truly verified through speakers: a paused worker is the only
  place that can do it, and it needs a human to click the tray icon.

### Seeing the session without asking

The Remote Desktop client's window cannot be captured on macOS, so the worker
takes its own screenshots: a capture runs once into the build log, then a
detached process saves the screen every fifteen seconds and posts it as a build
artifact. Fetch the newest one with:

```
scripts/vm-shot.sh        # newest screen -> .rdp/vm-live.jpg
```

Traps in that path, both fixed but easy to reintroduce:

- upload with `appveyor PushArtifact <file> -FileName <name>`, the worker's own
  CLI. `POST api/artifacts` answers with an object that holds no `url`, and
  `WebClient.UploadFile` cannot post to it, while the CLI worked first time.
- run the capture in a separate `powershell -EncodedCommand` process. Inside a
  `Start-Job` it failed without a trace, and the encoded form also sidesteps
  every quoting rule between YAML and PowerShell — but build the whole
  `Start-Process` line in one place: splicing the encoded string into a
  half-written argument list leaves a stray quote and PowerShell rejects it.
- download with `curl -L`: the artifact endpoint answers 302 and signs a storage
  URL, so without following the redirect the file arrives empty.

The capture runs before the RDP client connects, so the first picture is the
worker's 1024x768 desktop; later ones follow the session's real size.

### Branch

Prototype work lives on `flyout-prototype`; AppVeyor builds it on every push and
github.com/.../actions builds the downloadable zip (`Radio-windows-x86_64`).
