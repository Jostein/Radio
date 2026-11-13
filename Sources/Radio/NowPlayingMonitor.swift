import AVFoundation
import Foundation

/// Supplies the current track title for a stream.
///
/// Two mechanisms cover every station: polling a source (an ICY metadata block
/// inside the audio, or a station's JSON feed) and, for HLS, metadata the player
/// itself pushes. Only one source is active at a time, and a feed always wins
/// over reading the audio.
final class NowPlayingMonitor: NSObject {
    private(set) var title: String?
    var onChange: ((String?) -> Void)?

    /// One source being polled, with the policy for what to do when it fails.
    private struct Polling {
        let interval: TimeInterval
        let retryDelay: TimeInterval
        /// Attempts after a failed read before the next regular refresh.
        let retries: Int
        let fetch: (@escaping (TrackUpdate) -> Void) -> Void
        let cancel: () -> Void
        /// Called when the retries are spent; a feed falls back to the stream.
        /// ICY has nothing behind it, so it simply waits for the next refresh.
        let onGiveUp: (() -> Void)?
    }

    private static let icyRefreshInterval: TimeInterval = 30
    private static let icyRetryDelay: TimeInterval = 6
    private static let icyRetries = 3
    private static let feedRetryDelay: TimeInterval = 5
    private static let feedRetries = 2

    private let icyReader = ICYTitleReader()
    private let metadataOutput = AVPlayerItemMetadataOutput(identifiers: nil)

    private var currentURL: URL?
    /// Held rather than referenced weakly: several fallbacks need it again
    /// later, and a stream that outlives its item would strand them.
    private var streamItem: AVPlayerItem?
    private var metadataAttached = false

    private var polling: Polling?
    private var pollTimer: Timer?
    private var retryWork: DispatchWorkItem?
    private var retriesLeft = 0
    /// Results from a source that has since been replaced must be ignored.
    private var pollGeneration = 0

    override init() {
        super.init()
        metadataOutput.setDelegate(self, queue: .main)
    }

    deinit {
        pollTimer?.invalidate()
        icyReader.stop()
    }

    func start(for url: URL, item: AVPlayerItem) {
        stop()

        currentURL = url
        streamItem = item

        // A station's own JSON feed is far cheaper than reading metadata out of
        // the audio, so prefer it and keep the stream as the fallback.
        if let feed = NowPlayingFeeds.curated(for: url) {
            startFeed(feed)
            return
        }

        startStreamMetadata(for: url)
        discoverFeed(for: url)
    }

    /// `clearingTitle: false` keeps the last known track, so pausing leaves it
    /// visible instead of blanking the menu row.
    func stop(clearingTitle: Bool = true) {
        stopPolling()
        detachMetadata()
        streamItem = nil
        currentURL = nil

        if clearingTitle {
            setTitle(nil)
        }
    }

    // MARK: - Stream metadata

    /// HLS carries timed metadata that the player pushes; other streams carry an
    /// ICY block that has to be read out of the audio.
    private func startStreamMetadata(for url: URL) {
        guard url.pathExtension.lowercased() == "m3u8" else {
            startICY(for: url)
            return
        }

        guard let streamItem else { return }
        streamItem.add(metadataOutput)
        metadataAttached = true
    }

    private func detachMetadata() {
        if metadataAttached {
            streamItem?.remove(metadataOutput)
        }
        metadataAttached = false
    }

    private func startICY(for url: URL) {
        startPolling(
            interval: Self.icyRefreshInterval,
            retryDelay: Self.icyRetryDelay,
            retries: Self.icyRetries,
            fetch: { [icyReader] completion in
                icyReader.readTitle(from: url, completion: completion)
            },
            cancel: { [icyReader] in icyReader.stop() },
            onGiveUp: nil
        )
    }

    /// Looks for a feed the station publishes itself. Stream metadata is already
    /// running, so a slow or missing answer costs nothing and changes nothing.
    private func discoverFeed(for url: URL) {
        NowPlayingFeeds.discover(for: url) { [weak self] feed in
            DispatchQueue.main.async {
                guard let self, let feed, self.currentURL == url else { return }
                self.stopPolling()
                self.detachMetadata()
                self.startFeed(feed)
            }
        }
    }

    // MARK: - Feeds

    private func startFeed(_ feed: Feed) {
        startPolling(
            interval: feed.refreshInterval,
            retryDelay: Self.feedRetryDelay,
            retries: Self.feedRetries,
            fetch: { completion in Self.fetch(feed, completion: completion) },
            cancel: {},
            onGiveUp: { [weak self] in
                guard let self, let url = self.currentURL else { return }
                self.startStreamMetadata(for: url)
            }
        )
    }

    private static func fetch(_ feed: Feed, completion: @escaping (TrackUpdate) -> Void) {
        var request = URLRequest(url: feed.url)
        request.timeoutInterval = 8
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("Radio/1.0 (macOS)", forHTTPHeaderField: "User-Agent")

        URLSession.shared.dataTask(with: request) { data, response, error in
            let status = (response as? HTTPURLResponse)?.statusCode
            let update: TrackUpdate
            if error == nil, (status ?? 200) < 400, let data,
               let title = feed.parse(data), !title.isEmpty {
                update = .title(title)
            } else {
                update = .unavailable
            }

            DispatchQueue.main.async { completion(update) }
        }.resume()
    }

    // MARK: - Polling

    private func startPolling(
        interval: TimeInterval,
        retryDelay: TimeInterval,
        retries: Int,
        fetch: @escaping (@escaping (TrackUpdate) -> Void) -> Void,
        cancel: @escaping () -> Void,
        onGiveUp: (() -> Void)?
    ) {
        stopPolling()

        pollGeneration += 1
        polling = Polling(
            interval: interval,
            retryDelay: retryDelay,
            retries: retries,
            fetch: fetch,
            cancel: cancel,
            onGiveUp: onGiveUp
        )
        retriesLeft = retries
        poll()

        let timer = Timer(timeInterval: interval, repeats: true) { [weak self] _ in
            self?.poll()
        }
        RunLoop.main.add(timer, forMode: .common)
        pollTimer = timer
    }

    private func poll() {
        guard let polling else { return }
        let generation = pollGeneration

        polling.fetch { update in
            // Sources answer on their own queue. Timers, retries and the menu
            // updates that follow all belong on the main thread; AppKit aborts
            // if it is touched from anywhere else.
            Self.onMain { [self] in
                guard pollGeneration == generation else { return }
                handle(update)
            }
        }
    }

    private func handle(_ update: TrackUpdate) {
        switch update {
        case .title(let title):
            retriesLeft = polling?.retries ?? 0
            setTitle(title)
        case .noTitle:
            // News, ads or a jingle: the station is publishing no song, so drop
            // the last one instead of showing it indefinitely.
            retriesLeft = polling?.retries ?? 0
            setTitle(nil)
        case .unavailable:
            // Busy or slow servers often answer on the next attempt; without
            // this a station shows no track until the next refresh.
            retryOrGiveUp()
        }
    }

    private func retryOrGiveUp() {
        guard let polling else { return }

        if retriesLeft > 0 {
            retriesLeft -= 1
            guard retryWork == nil else { return }

            let work = DispatchWorkItem { [weak self] in
                guard let self else { return }
                self.retryWork = nil
                self.poll()
            }
            retryWork = work
            DispatchQueue.main.asyncAfter(deadline: .now() + polling.retryDelay, execute: work)
            return
        }

        guard let giveUp = polling.onGiveUp else { return }
        stopPolling()
        giveUp()
    }

    private func stopPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
        retryWork?.cancel()
        retryWork = nil
        polling?.cancel()
        polling = nil
        retriesLeft = 0
    }

    private static func onMain(_ work: @escaping () -> Void) {
        if Thread.isMainThread {
            work()
        } else {
            DispatchQueue.main.async(execute: work)
        }
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
