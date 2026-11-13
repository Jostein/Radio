import AVFoundation
import AppKit
import Foundation

final class RadioPlayer {
    enum State {
        case idle
        case connecting
        case playing
        case paused
        case reconnecting(attempt: Int)
        case failed(String)
    }

    static let maxReconnectAttempts = 6

    private(set) var current: Station?
    private(set) var state: State = .idle

    var trackTitle: String? {
        nowPlaying.title
    }

    var onChange: (() -> Void)?

    /// Full volume by default; remembered between launches.
    var volume: Double {
        didSet {
            avPlayer.volume = Float(volume)
            UserDefaults.standard.set(volume, forKey: Self.volumeKey)
        }
    }

    /// True while the player is trying to produce audio, including retries.
    /// This is also the playback intent: pausing leaves `.paused`/`.idle`.
    var isActive: Bool {
        switch state {
        case .connecting, .playing, .reconnecting: return true
        case .idle, .paused, .failed: return false
        }
    }

    private static let volumeKey = "volume"

    /// Created on first use: constructing a player loads AVFoundation, which a
    /// menu bar app that never plays would otherwise pay for at launch.
    private lazy var avPlayer = AVPlayer()

    /// Bumped whenever the target stream changes, so a slow playlist lookup
    /// can't start playback for a station the user already left.
    private var generation = 0

    private var statusObservation: NSKeyValueObservation?
    private var failureObserver: NSObjectProtocol?
    private var wakeObserver: NSObjectProtocol?

    private var reconnectWorkItem: DispatchWorkItem?
    private var reconnectAttempt = 0

    private var healthTimer: Timer?
    private var stalledTicks = 0
    private var healthyTicks = 0

    private let nowPlaying = NowPlayingMonitor()

    /// The resolved stream URL, once a playlist has been unwrapped. Distinct
    /// from `Station.streamURL`, which is whatever the user saved.
    private var resolvedURL: URL?

    /// Watchdog cadence, and how many consecutive unhealthy polls before the
    /// stream is rebuilt.
    private let tickInterval: TimeInterval = 5
    private let ticksBeforeReconnect = 4

    /// Healthy polls needed before a fresh failure is treated as a new problem
    /// rather than a continuation of the last one.
    private let healthyTicksBeforeRecovery = 6

    init() {
        let stored = UserDefaults.standard.object(forKey: Self.volumeKey) as? Double
        volume = min(max(stored ?? 1, 0), 1)

        nowPlaying.onChange = { [weak self] _ in
            self?.onChange?()
        }

        failureObserver = NotificationCenter.default.addObserver(
            forName: .AVPlayerItemFailedToPlayToEndTime,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            let error = notification.userInfo?[AVPlayerItemFailedToPlayToEndTimeErrorKey] as? Error
            DispatchQueue.main.async {
                self?.handleFailure(error?.localizedDescription ?? "The stream stopped unexpectedly.")
            }
        }

        // Sleep freezes the socket and the watchdog, so treat waking as a fresh
        // tune-in rather than trusting a connection that sat idle for hours.
        wakeObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didWakeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.recoverAfterWake()
        }
    }

    deinit {
        nowPlaying.stop()
        if let failureObserver {
            NotificationCenter.default.removeObserver(failureObserver)
        }
        if let wakeObserver {
            NSWorkspace.shared.notificationCenter.removeObserver(wakeObserver)
        }
    }

    // MARK: - Playback

    func play(_ station: Station) {
        cancelRecovery()
        nowPlaying.stop()
        reconnectAttempt = 0
        current = station

        guard let url = station.streamURL else {
            generation += 1
            detachItem()
            resolvedURL = nil
            state = .failed("That stream URL isn’t valid.")
            onChange?()
            return
        }

        begin(url, isRetry: false)
    }

    func togglePlayback() {
        guard let current else { return }
        if isActive {
            pause()
        } else if let item = avPlayer.currentItem, item.status == .readyToPlay {
            resume()
        } else {
            play(current)
        }
    }

    func pause() {
        generation += 1
        cancelRecovery()
        // Keep the track that was playing on screen while paused.
        nowPlaying.stop(clearingTitle: false)
        avPlayer.pause()
        state = current == nil ? .idle : .paused
        onChange?()
    }

    func stop() {
        generation += 1
        cancelRecovery()
        nowPlaying.stop()
        avPlayer.pause()
        detachItem()
        current = nil
        resolvedURL = nil
        reconnectAttempt = 0
        state = .idle
        onChange?()
    }

    private func resume() {
        avPlayer.play()
        stalledTicks = 0
        healthyTicks = 0
        state = .playing
        startHealthTimer()

        // Pausing tore the metadata reader down; bring it back for the same item.
        if let resolvedURL, let item = avPlayer.currentItem {
            nowPlaying.start(for: resolvedURL, item: item)
        }

        onChange?()
    }

    // MARK: - Stream loading

    /// Starts a connection attempt, resolving a playlist first when needed.
    private func begin(_ url: URL, isRetry: Bool) {
        generation += 1
        let token = generation

        state = isRetry ? .reconnecting(attempt: reconnectAttempt) : .connecting
        startHealthTimer()
        onChange?()

        guard PlaylistResolver.isPlaylist(url) else {
            load(url)
            return
        }

        // Silence the old stream while the playlist is being read, so its late
        // callbacks can't be mistaken for the attempt we are about to make.
        detachItem()

        PlaylistResolver.resolve(url) { [weak self] result in
            guard let self, token == self.generation, self.isActive else { return }
            switch result {
            case .success(let streamURL):
                self.load(streamURL)
            case .failure(let error):
                self.handleFailure(error.localizedDescription)
            }
        }
    }

    private func load(_ url: URL) {
        statusObservation = nil
        stalledTicks = 0
        healthyTicks = 0

        let item = AVPlayerItem(url: url)
        // A live stream never seeks, so the default forward buffer only costs
        // memory and start-up latency. Measured on this Mac, with the same
        // station: 16 MB footprint at the default buffer, 13 MB at six seconds.
        item.preferredForwardBufferDuration = 6
        avPlayer.automaticallyWaitsToMinimizeStalling = false
        observe(item)
        avPlayer.replaceCurrentItem(with: item)
        avPlayer.volume = Float(volume)
        avPlayer.play()

        resolvedURL = url
        nowPlaying.start(for: url, item: item)
    }

    private func observe(_ item: AVPlayerItem) {
        statusObservation = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            DispatchQueue.main.async {
                guard let self, item === self.avPlayer.currentItem else { return }
                switch item.status {
                case .readyToPlay:
                    // A retry that lands after the user hit Pause must not
                    // flip the app back into a playing state.
                    guard self.isActive else { return }
                    self.stalledTicks = 0
                    self.state = .playing
                    self.onChange?()
                case .failed:
                    self.handleFailure(item.error?.localizedDescription ?? "This stream couldn’t be played.")
                default:
                    break
                }
            }
        }
    }

    private func detachItem() {
        statusObservation = nil
        avPlayer.replaceCurrentItem(with: nil)
    }

    // MARK: - Reconnecting

    private func handleFailure(_ message: String) {
        guard isActive, reconnectWorkItem == nil else { return }
        stopHealthTimer()
        nowPlaying.stop()

        guard let url = current?.streamURL, reconnectAttempt < Self.maxReconnectAttempts else {
            state = .failed(message)
            onChange?()
            return
        }

        reconnectAttempt += 1
        state = .reconnecting(attempt: reconnectAttempt)
        onChange?()

        let delay = min(pow(2, Double(reconnectAttempt - 1)), 30)
        scheduleReconnect(to: url, after: delay)
    }

    private func scheduleReconnect(to url: URL, after delay: TimeInterval) {
        reconnectWorkItem?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.reconnectWorkItem = nil
            guard self.isActive, self.current?.streamURL == url else { return }
            self.begin(url, isRetry: true)
        }
        reconnectWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + delay, execute: work)
    }

    /// Cancels any scheduled retry and stops the watchdog.
    private func cancelRecovery() {
        reconnectWorkItem?.cancel()
        reconnectWorkItem = nil
        stopHealthTimer()
    }

    private func recoverAfterWake() {
        guard isActive, let url = current?.streamURL else { return }
        reconnectAttempt = 0
        begin(url, isRetry: false)
    }

    // MARK: - Health watchdog

    private func startHealthTimer() {
        guard healthTimer == nil else { return }
        let timer = Timer(timeInterval: tickInterval, repeats: true) { [weak self] _ in
            self?.checkHealth()
        }
        RunLoop.main.add(timer, forMode: .common)
        healthTimer = timer
    }

    private func stopHealthTimer() {
        healthTimer?.invalidate()
        healthTimer = nil
        stalledTicks = 0
        healthyTicks = 0
    }

    private func checkHealth() {
        guard isActive else { return }

        if avPlayer.timeControlStatus == .playing {
            stalledTicks = 0
            healthyTicks += 1
            // Only forgive earlier failures after sustained healthy playback, so
            // a stream that drops right after connecting still backs off.
            if healthyTicks >= healthyTicksBeforeRecovery {
                reconnectAttempt = 0
            }
            return
        }

        healthyTicks = 0
        stalledTicks += 1
        if stalledTicks >= ticksBeforeReconnect {
            handleFailure("The stream stopped responding.")
        }
    }
}
