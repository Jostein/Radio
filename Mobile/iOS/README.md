# Radio for iOS

A native SwiftUI iPhone/iPad player. It uses the same stream rules and metadata
code as the desktop app: HLS and progressive streams, `.m3u`/`.pls` resolution,
ICY/feed titles, reconnect backoff and a stall watchdog.

## Requirements

- Xcode 16 or newer
- iOS 17 or newer
- XcodeGen only if you want to regenerate the project

The Xcode project is committed, so opening `Radio.xcodeproj` is enough. To
regenerate it after changing `project.yml`:

```sh
brew install xcodegen
cd Mobile/iOS
xcodegen generate
open Radio.xcodeproj
```

Select a simulator and Run. For a physical device or TestFlight, choose your
development team in Signing & Capabilities first.

## What works

- Station list with add and swipe-to-delete
- Play/pause from the list or the now-playing card
- Background audio and lock-screen / Control Center controls
- Track title from HLS metadata, ICY or the station feed
- Reconnect when a stream drops
- The same starter station and `stations.json` model as the desktop builds

## Not yet

- Find Station search
- An in-app volume slider (the hardware/system volume is used)
- App Store archive, signing and TestFlight

## CI

`.github/workflows/ios.yml` builds the simulator app on a macOS runner and
launches it in an iPhone simulator. It deliberately disables code signing; an
App Store build needs an Apple Developer account and a real signing identity.
