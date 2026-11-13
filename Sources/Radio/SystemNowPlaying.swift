import Foundation
import MediaPlayer

/// Mirrors playback into the system's Now Playing panel, so Control Center,
/// the media keys and AirPods controls drive the radio like any other player.
final class SystemNowPlaying {
    private let center = MPNowPlayingInfoCenter.default()

    func install(player: RadioPlayer) {
        let commands = MPRemoteCommandCenter.shared()

        commands.playCommand.addTarget { [weak player] _ in
            guard let player else { return .commandFailed }
            DispatchQueue.main.async { if !player.isActive { player.togglePlayback() } }
            return .success
        }

        commands.pauseCommand.addTarget { [weak player] _ in
            guard let player else { return .commandFailed }
            DispatchQueue.main.async { if player.isActive { player.togglePlayback() } }
            return .success
        }

        commands.togglePlayPauseCommand.addTarget { [weak player] _ in
            guard let player else { return .commandFailed }
            DispatchQueue.main.async { player.togglePlayback() }
            return .success
        }

        // Live radio has no track to skip.
        commands.nextTrackCommand.isEnabled = false
        commands.previousTrackCommand.isEnabled = false
    }

    /// The track is the title and the station the artist, which is how the panel
    /// reads without inventing a split in "Artist — Title" strings.
    func update(station: String?, track: String?, isPlaying: Bool) {
        guard station != nil || track != nil else {
            center.nowPlayingInfo = nil
            center.playbackState = .stopped
            return
        }

        var info: [String: Any] = [:]
        info[MPMediaItemPropertyTitle] = track ?? station ?? "Radio"
        if let station {
            info[MPMediaItemPropertyArtist] = station
        }
        info[MPNowPlayingInfoPropertyIsLiveStream] = true
        info[MPNowPlayingInfoPropertyPlaybackRate] = isPlaying ? 1.0 : 0.0

        center.nowPlayingInfo = info
        center.playbackState = isPlaying ? .playing : .paused
    }
}
