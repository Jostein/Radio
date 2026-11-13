import AVFoundation
import Foundation

/// Supplies the current track title for a stream.
///
/// Icecast/Shoutcast stations carry it as ICY metadata inside the audio, which
/// has to be polled; HLS stations carry it as timed metadata on the player item.
final class NowPlayingMonitor: NSObject {
    private(set) var title: String?
    var onChange: ((String?) -> Void)?

    private let icyReader = ICYTitleReader()
    private var icyTimer: Timer?
    private var icyURL: URL?
    private var retryWorkItem: DispatchWorkItem?
    private var retriesLeft = 0
    private let icyRefreshInterval: TimeInterval = 30
    private let icyRetryDelay: TimeInterval = 6
    private static let maxICYRetries = 3

    private let metadataOutput = AVPlayerItemMetadataOutput(identifiers: nil)
    private weak var metadataItem: AVPlayerItem?

    override init() {
        super.init()
        metadataOutput.setDelegate(self, queue: .main)
    }

    deinit {
        icyTimer?.invalidate()
        icyReader.stop()
    }

    func start(for url: URL, item: AVPlayerItem) {
        stop()

        guard url.pathExtension.lowercased() == "m3u8" else {
            startICYUpdates(for: url)
            return
        }

        metadataItem = item
        item.add(metadataOutput)
    }

    /// `clearingTitle: false` keeps the last known track, so pausing leaves it
    /// visible instead of blanking the menu row.
    func stop(clearingTitle: Bool = true) {
        icyTimer?.invalidate()
        icyTimer = nil
        icyURL = nil
        retriesLeft = 0
        retryWorkItem?.cancel()
        retryWorkItem = nil
        icyReader.stop()

        if let metadataItem {
            metadataItem.remove(metadataOutput)
        }
        metadataItem = nil

        if clearingTitle {
            setTitle(nil)
        }
    }

    // MARK: - ICY polling

    private func startICYUpdates(for url: URL) {
        icyURL = url
        retriesLeft = Self.maxICYRetries
        pollICY()

        let timer = Timer(timeInterval: icyRefreshInterval, repeats: true) { [weak self] _ in
            self?.pollICY()
        }
        RunLoop.main.add(timer, forMode: .common)
        icyTimer = timer
    }

    private func pollICY() {
        guard let url = icyURL else { return }

        icyReader.readTitle(from: url) { [weak self] result in
            // The reader answers on its URLSession queue. Timers, retries and
            // the menu updates that follow from this all belong on the main
            // thread; AppKit aborts if it is touched from anywhere else.
            Self.onMain { self?.handleICY(result, for: url) }
        }
    }

    private func handleICY(_ result: ICYTitleReader.Result, for url: URL) {
        guard icyURL == url else { return }

        switch result {
        case .title(let title):
            retriesLeft = Self.maxICYRetries
            setTitle(title)
        case .empty:
            // News, ads or a jingle: the station is publishing no song, so drop
            // the last one instead of showing it indefinitely.
            retriesLeft = Self.maxICYRetries
            setTitle(nil)
        case .unavailable:
            // Busy or slow servers often answer on the next attempt; without
            // this a station shows no track until the 30s refresh.
            scheduleICYRetry(for: url)
        }
    }

    private static func onMain(_ work: @escaping () -> Void) {
        if Thread.isMainThread {
            work()
        } else {
            DispatchQueue.main.async(execute: work)
        }
    }

    private func scheduleICYRetry(for url: URL) {
        guard retriesLeft > 0, retryWorkItem == nil else { return }
        retriesLeft -= 1

        let work = DispatchWorkItem { [weak self] in
            guard let self, self.icyURL == url else { return }
            self.retryWorkItem = nil
            self.pollICY()
        }
        retryWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + icyRetryDelay, execute: work)
    }

    // MARK: - Publishing

    private func setTitle(_ newTitle: String?) {
        guard newTitle != title else { return }
        title = newTitle
        onChange?(newTitle)
    }
}

// MARK: - HLS timed metadata

extension NowPlayingMonitor: AVPlayerItemMetadataOutputPushDelegate {
    func metadataOutput(
        _ output: AVPlayerItemMetadataOutput,
        didOutputTimedMetadataGroups groups: [AVTimedMetadataGroup],
        from track: AVPlayerItemTrack?
    ) {
        guard let detected = Self.trackTitle(in: groups) else { return }
        setTitle(detected)
    }

    private static func trackTitle(in groups: [AVTimedMetadataGroup]) -> String? {
        for group in groups {
            var title: String?
            var artist: String?

            for item in group.items {
                guard let value = (item.value as? String)?.trimmingCharacters(in: .whitespacesAndNewlines),
                      !value.isEmpty
                else { continue }

                switch item.commonKey {
                case .some(.commonKeyTitle):
                    title = value
                case .some(.commonKeyArtist), .some(.commonKeyAuthor):
                    artist = artist ?? value
                default:
                    // Raw ID3 frames, when AVFoundation does not map them.
                    switch (item.key as? String)?.lowercased() {
                    case "tit2", "tt2", "inam", "\u{00a9}nam", "streamtitle":
                        title = value
                    case "tpe1", "tp1", "iart", "\u{00a9}art":
                        artist = artist ?? value
                    default:
                        break
                    }
                }
            }

            if let title, let artist, !artist.isEmpty, artist != title {
                return "\(artist) — \(title)"
            }
            if let title { return title }
            if let artist { return artist }
        }

        return nil
    }
}
