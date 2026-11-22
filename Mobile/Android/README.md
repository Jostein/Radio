# Radio for Android and Android TV

A native Kotlin/Jetpack Compose phone, tablet and TV player. Playback runs in a
Media3 `MediaSessionService`, so it keeps playing when the app is not on screen
and appears in the notification shade, lock screen and system media controls.

## Requirements

- Android Studio (or JDK 17+ and the Android SDK)
- Android SDK platform 37.0 and build-tools 36.0.0
- Android 8.0 (API 26) or newer

The Gradle wrapper is committed. Open `Mobile/Android` in Android Studio, or
build from a terminal:

```sh
cd Mobile/Android
./gradlew :app:assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Debug builds are signed with the local debug key. A Play Store build needs a
release signing key and a signed `.aab`; neither is configured yet.

## What works

- Station list with add and delete
- Direct HLS (`.m3u8`) and progressive HTTP streams
- Play/pause from the list or the now-playing card
- Background audio through a Media3 `MediaSessionService`
- Notification, lock-screen and headset media controls
- A track title whenever the stream reports one through Media3
- The same FIP Reggae starter station as the desktop and iOS apps
- Android TV launcher entry, banner and visible D-pad focus on cards and buttons

## Not yet

- Find Station search
- `.m3u`/`.pls` playlist resolution
- The desktop/iOS reconnect backoff and stall watchdog; Media3's default retry
  handling is what plays a dropped stream now
- An in-app volume slider (the system volume is used)
- A TV runtime test; the same APK has the Leanback entry and D-pad focus, but
  it has not been launched on a TV or TV emulator
- Play Store signing, a release bundle, or a device/emulator run in CI

## Android TV

The TV version is the same APK, not a separate app. The manifest declares
`android.software.leanback`, a `LEANBACK_LAUNCHER` entry and a 320×180 banner;
the Compose cards and buttons show a focus border when navigated with a remote.

The single-column UI is deliberately shared with the phone build. Add a station
with the on-screen keyboard, then use the D-pad to select and play it.

## Verification

`./gradlew :app:assembleDebug` passes locally against Android SDK 37.0. The
GitHub Actions workflow in `.github/workflows/android.yml` builds the same APK
on a clean Linux machine. The app has not yet been launched on a physical device,
TV or emulator, so the notification, background-audio and TV-focus paths are
compiled but not exercised here.
