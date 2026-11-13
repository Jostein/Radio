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
    private var monitoredURL: URL?
    private var retryWorkItem: DispatchWorkItem?
    private var retriesLeft = 0
    private let icyRefreshInterval: TimeInterval = 30
    private let icyRetryDelay: TimeInterval = 6
    private static let maxICYRetries = 3

    private let metadataOutput = AVPlayerItemMetadataOutput(identifiers: nil)
    private weak var metadataItem: AVPlayerItem?
    private weak var playerItem: AVPlayerItem?

    private var apiTimer: Timer?
    private var apiSource: NowPlayingAPI.Source?
    private var apiStreamURL: URL?
    private var apiFailures = 0
    private static let apiMaxFailures = 3

    override init() {
        super.init()
        metadataOutput.setDelegate(self, queue: .main)
    }

    deinit {
        icyTimer?.invalidate()
        apiTimer?.invalidate()
        icyReader.stop()
    }

    func start(for url: URL, item: AVPlayerItem) {
        stop()

        playerItem = item
        monitoredURL = url

        // A station's own JSON feed is far cheaper than reading metadata out of
        // the audio, so prefer it and keep the stream as the fallback.
        if let source = NowPlayingAPI.source(for: url) {
            startAPIUpdates(using: source, for: url)
            return
        }

        startStreamUpdates(for: url)
        discoverFeed(for: url)
    }

    /// Looks for a feed the station publishes itself. Stream metadata is already
    /// running, so a slow or missing answer costs nothing and changes nothing.
    private func discoverFeed(for url: URL) {
        NowPlayingDiscovery.discover(for: url) { [weak self] source in
            DispatchQueue.main.async {
                guard let self, let source, self.monitoredURL == url else { return }
                self.adopt(source, for: url)
            }
        }
    }

    /// Switches from the audio stream's metadata to a discovered feed.
    private func adopt(_ source: NowPlayingAPI.Source, for url: URL) {
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

        startAPIUpdates(using: source, for: url)
    }

    /// HLS carries timed metadata; other streams carry it inline as ICY blocks.
    private func startStreamUpdates(for url: URL) {
        guard url.pathExtension.lowercased() == "m3u8" else {
            startICYUpdates(for: url)
            return
        }

        guard let playerItem else { return }
        metadataItem = playerItem
        playerItem.add(metadataOutput)
    }

    /// `clearingTitle: false` keeps the last known track, so pausing leaves it
    /// visible instead of blanking the menu row.
    func stop(clearingTitle: Bool = true) {
        icyTimer?.invalidate()
        icyTimer = nil
        icyURL = nil
        monitoredURL = nil
        retriesLeft = 0
        retryWorkItem?.cancel()
        retryWorkItem = nil
        stopAPIPolling()
        icyReader.stop()

        if let metadataItem {
            metadataItem.remove(metadataOutput)
        }
        metadataItem = nil

        playerItem = nil

        if clearingTitle {
            setTitle(nil)
        }
    }

    // MARK: - JSON feeds

    private func startAPIUpdates(using source: NowPlayingAPI.Source, for url: URL) {
        apiSource = source
        apiStreamURL = url
        apiFailures = 0
        pollAPI()

        let timer = Timer(timeInterval: source.refreshInterval, repeats: true) { [weak self] _ in
            self?.pollAPI()
        }
        RunLoop.main.add(timer, forMode: .common)
        apiTimer = timer
    }

    private func pollAPI() {
        guard let source = apiSource else { return }

        var request = URLRequest(url: source.url)
        request.timeoutInterval = 8
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("Radio/1.0 (macOS)", forHTTPHeaderField: "User-Agent")

        URLSession.shared.dataTask(with: request) { [weak self] data, response, error in
            let title = data.flatMap(source.parse)
            let status = (response as? HTTPURLResponse)?.statusCode

            DispatchQueue.main.async {
                guard let self, self.apiSource?.url == source.url else { return }

                if let title, !title.isEmpty {
                    self.apiFailures = 0
                    self.setTitle(title)
                } else {
                    self.noteAPIFailure(status: status, error: error)
                }
            }
        }.resume()
    }

    /// A feed that answers with an error, or keeps failing, is abandoned for
    /// this stream and the audio stream's own metadata takes over.
    private func noteAPIFailure(status: Int?, error: Error?) {
        apiFailures += 1
        let gone = (status ?? 200) >= 400
        guard gone || apiFailures >= Self.apiMaxFailures, let url = apiStreamURL else { return }

        stopAPIPolling()
        startStreamUpdates(for: url)
    }

    private func stopAPIPolling() {
        apiTimer?.invalidate()
        apiTimer = nil
        apiSource = nil
        apiStreamURL = nil
        apiFailures = 0
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
