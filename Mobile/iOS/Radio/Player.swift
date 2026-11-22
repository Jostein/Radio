import AVFoundation
import Foundation

/// The iOS playback engine. It follows the same stream rules as the desktop
/// build, but owns the iOS audio session so playback survives the lock screen.
final class Player {
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

    var volume: Double {
        didSet {
            avPlayer.volume = Float(volume)
            UserDefaults.standard.set(volume, forKey: Self.volumeKey)
        }
    }

    var isActive: Bool {
        switch state {
        case .connecting, .playing, .reconnecting: return true
        case .idle, .paused, .failed: return false
        }
    }

    var statusDescription: String {
        switch state {
        case .idle:
            return "Nothing playing"
        case .connecting:
            return "Connecting…"
        case .playing:
            return "Live"
        case .paused:
            return "Paused"
        case .reconnecting(let attempt):
            return "Reconnecting (\(attempt) of \(Self.maxReconnectAttempts))…"
        case .failed(let message):
            return message
        }
    }

    private static let volumeKey = "volume"
    private lazy var avPlayer = AVPlayer()
    private let nowPlaying = NowPlayingMonitor()

    /// Bumped whenever the target changes so a late playlist response cannot
    /// start a station the user has already left.
    private var generation = 0
    private var statusObservation: NSKeyValueObservation?
    private var failureObserver: NSObjectProtocol?
    private var endObserver: NSObjectProtocol?

    private var reconnectWorkItem: DispatchWorkItem?
    private var reconnectAttempt = 0

    private var healthTimer: Timer?
    private var lastPosition: Double = 0
    private var stalledTicks = 0
    private var hasPlayed = false
    private let tickInterval: TimeInterval = 5
    private let ticksBeforeReconnect = 4

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

        endObserver = NotificationCenter.default.addObserver(
            forName: .AVPlayerItemDidPlayToEndTime,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self, let item = notification.object as? AVPlayerItem,
                  item === self.avPlayer.currentItem else { return }
            DispatchQueue.main.async { self.handleFailure("The stream ended.") }
        }
    }

    deinit {
        nowPlaying.stop()
        if let failureObserver {
            NotificationCenter.default.removeObserver(failureObserver)
        }
        if let endObserver {
            NotificationCenter.default.removeObserver(endObserver)
        }
    }

    // MARK: - Playback

    func play(_ station: Station) {
        cancelRecovery()
        nowPlaying.stop()
        reconnectAttempt = 0
        current = station
        activateAudioSession()

        guard let url = station.streamURL else {
            generation += 1
            detachItem()
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
        } else {
            play(current)
        }
    }

    func pause() {
        generation += 1
        cancelRecovery()
        nowPlaying.stop(clearingTitle: false)
        avPlayer.pause()
        detachItem()
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
        reconnectAttempt = 0
        state = .idle
        onChange?()
    }

    // MARK: - Stream loading

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
        lastPosition = 0
        hasPlayed = false

        let item = AVPlayerItem(url: url)
        if url.path.lowercased().hasSuffix(".m3u8") {
            item.preferredForwardBufferDuration = 6
        }
        observe(item)
        avPlayer.replaceCurrentItem(with: item)
        avPlayer.volume = Float(volume)
        avPlayer.play()
        nowPlaying.start(for: url, item: item)
    }

    private func observe(_ item: AVPlayerItem) {
        statusObservation = item.observe(\.status, options: [.new]) { [weak self] item, _ in
            DispatchQueue.main.async {
                guard let self, item === self.avPlayer.currentItem else { return }
                switch item.status {
                case .readyToPlay:
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

    private func cancelRecovery() {
        reconnectWorkItem?.cancel()
        reconnectWorkItem = nil
        stopHealthTimer()
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
    }

    private func checkHealth() {
        guard isActive else { return }

        if avPlayer.timeControlStatus == .paused && hasPlayed {
            handleFailure("The stream ended.")
            return
        }

        let seconds = avPlayer.currentTime().seconds
        let moved = seconds.isFinite && seconds > lastPosition + 0.5
        if seconds.isFinite {
            lastPosition = seconds
        }

        if avPlayer.timeControlStatus == .playing && moved {
            hasPlayed = true
            stalledTicks = 0
            return
        }

        stalledTicks += 1
        if stalledTicks >= ticksBeforeReconnect {
            handleFailure("The stream stopped responding.")
        }
    }

    // MARK: - iOS audio

    private func activateAudioSession() {
        let session = AVAudioSession.sharedInstance()
        try? session.setCategory(.playback, mode: .default, options: [])
        try? session.setActive(true)
    }
}
