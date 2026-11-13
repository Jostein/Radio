import Foundation

/// A JSON document that names the current track.
///
/// Reading one costs a few hundred bytes to a few kilobytes, against the
/// `icy-metaint` block of audio a stream read needs, so a feed is both cheaper
/// and fresher than the stream it belongs to.
struct Feed {
    let url: URL
    let refreshInterval: TimeInterval
    let parse: (Data) -> String?
}

enum NowPlayingFeeds {
    /// Feeds are found two ways: broadcasters whose endpoints are known and
    /// stable, and servers that expose a generic status document. Everything
    /// else keeps using the stream's own metadata.
    static func curated(for streamURL: URL) -> Feed? {
        guard let host = streamURL.host?.lowercased() else { return nil }

        if host.hasSuffix("somafm.com") {
            return somaFM(streamURL)
        }
        if host.contains("radioparadise.com") {
            return radioParadise(streamURL)
        }
        if host.contains("radiofrance.fr"), streamURL.absoluteString.lowercased().contains("fip") {
            return radioFrance(streamURL)
        }
        return nil
    }

    // MARK: - SomaFM

    /// `/groovesalad-128-mp3` is channel `groovesalad`.
    private static func somaFM(_ streamURL: URL) -> Feed? {
        guard let mount = streamURL.pathComponents.dropFirst().first else { return nil }
        let channel = mount.replacingOccurrences(
            of: "-[0-9]+-(mp3|aac[a-z0-9-]*)$",
            with: "",
            options: [.regularExpression, .caseInsensitive]
        )
        guard !channel.isEmpty, let url = URL(string: "https://somafm.com/songs/\(channel).json") else {
            return nil
        }

        return Feed(url: url, refreshInterval: 10) { data in
            guard
                let root = jsonObject(in: data),
                let songs = root["songs"] as? [[String: Any]],
                let current = songs.first
            else { return nil }
            return join(current["artist"] as? String, current["title"] as? String)
        }
    }

    // MARK: - Radio Paradise

    private static func radioParadise(_ streamURL: URL) -> Feed? {
        let path = streamURL.absoluteString.lowercased()
        let channel: Int
        switch true {
        case path.contains("mellow"): channel = 1
        case path.contains("rock"): channel = 2
        case path.contains("global"), path.contains("world"): channel = 3
        default: channel = 0
        }

        guard let url = URL(string: "https://api.radioparadise.com/api/now_playing?chan=\(channel)") else {
            return nil
        }

        return Feed(url: url, refreshInterval: 10) { data in
            guard let root = jsonObject(in: data) else { return nil }
            return join(root["artist"] as? String, root["title"] as? String)
        }
    }

    // MARK: - Radio France (FIP family)

    /// livemeta station ids for the FIP webradios, most specific slug first.
    private static let fipStations: [(slug: String, id: Int)] = [
        ("fipreggae", 71),
        ("fiprock", 64),
        ("fipjazz", 65),
        ("fipgroove", 66),
        ("fipelectro", 74),
        ("fipmetal", 77),
        ("fipnouveau", 70),
        ("fipworld", 69),
        ("fipmonde", 69)
    ]

    private static func radioFrance(_ streamURL: URL) -> Feed? {
        let address = streamURL.absoluteString.lowercased()
        let id = fipStations.first { address.contains($0.slug) }?.id ?? 7
        guard let url = URL(string: "https://api.radiofrance.fr/livemeta/pull/\(id)") else {
            return nil
        }

        // Radio France spends a few kilobytes on the schedule around the song,
        // so this one is polled less often and still beats the audio-block read.
        return Feed(url: url, refreshInterval: 20) { data in
            guard
                let root = jsonObject(in: data),
                let levels = root["levels"] as? [[String: Any]],
                let level = levels.first,
                let items = level["items"] as? [Any],
                let position = level["position"] as? Int,
                items.indices.contains(position),
                let steps = root["steps"]
            else { return nil }

            let identifier = items[position]
            let step: [String: Any]?
            if let table = steps as? [String: Any] {
                step = table[String(describing: identifier)] as? [String: Any]
            } else if let list = steps as? [[String: Any]], let index = identifier as? Int {
                step = list.indices.contains(index) ? list[index] : nil
            } else {
                step = nil
            }

            guard let step else { return nil }
            return join(step["authors"] as? String, step["title"] as? String)
        }
    }

    // MARK: - Discovery

    /// An ICY read costs 8–64 KB, so a bigger document would be a downgrade.
    private static let maximumFeedBytes = 16_384

    /// Looks for a status document this server publishes. Two conventions exist:
    /// Icecast answers on `/status-json.xsl` for the whole server, Shoutcast on
    /// `/stats?json=1`. A feed is adopted only when it names a title for this
    /// mount and is small enough that polling it beats reading the audio.
    static func discover(for streamURL: URL, completion: @escaping (Feed?) -> Void) {
        icecastFeed(for: streamURL) { feed in
            if let feed {
                completion(feed)
                return
            }
            shoutcastFeed(for: streamURL, completion: completion)
        }
    }

    private static func icecastFeed(for streamURL: URL, completion: @escaping (Feed?) -> Void) {
        guard let url = endpoint(for: streamURL, path: "/status-json.xsl") else {
            completion(nil)
            return
        }
        let mount = streamURL.lastPathComponent

        fetch(url) { data in
            guard let data, icecastTitle(in: data, mount: mount) != nil else {
                completion(nil)
                return
            }
            completion(
                Feed(url: url, refreshInterval: interval(for: data)) { data in
                    icecastTitle(in: data, mount: mount)
                }
            )
        }
    }

    private static func icecastTitle(in data: Data, mount: String) -> String? {
        guard
            let root = jsonObject(in: data),
            let stats = root["icestats"] as? [String: Any]
        else { return nil }

        let sources: [[String: Any]]
        if let list = stats["source"] as? [[String: Any]] {
            sources = list
        } else if let single = stats["source"] as? [String: Any] {
            sources = [single]
        } else {
            return nil
        }

        let target = mount.lowercased()
        for source in sources {
            let listen = (source["listenurl"] as? String).flatMap { URL(string: $0)?.lastPathComponent }
            guard listen?.lowercased() == target else { continue }
            if let title = clean(source["title"] as? String) {
                return title
            }
        }

        // A server with exactly one mount is this station, whatever it calls it.
        if sources.count == 1 {
            return clean(sources[0]["title"] as? String)
        }
        return nil
    }

    private static func shoutcastFeed(for streamURL: URL, completion: @escaping (Feed?) -> Void) {
        guard let url = endpoint(for: streamURL, path: "/stats", query: "json=1") else {
            completion(nil)
            return
        }

        fetch(url) { data in
            guard let data, shoutcastTitle(in: data) != nil else {
                completion(nil)
                return
            }
            completion(
                Feed(url: url, refreshInterval: interval(for: data)) { data in
                    shoutcastTitle(in: data)
                }
            )
        }
    }

    private static func shoutcastTitle(in data: Data) -> String? {
        guard let root = jsonObject(in: data) else { return nil }

        for key in ["songtitle", "currentsong", "title"] {
            if let title = clean(root[key] as? String) {
                return title
            }
        }
        return nil
    }

    // MARK: - Helpers

    private static func endpoint(for streamURL: URL, path: String, query: String? = nil) -> URL? {
        guard var components = URLComponents(url: streamURL, resolvingAgainstBaseURL: false) else { return nil }
        components.path = path
        components.query = query
        components.fragment = nil
        return components.url
    }

    private static func fetch(_ url: URL, completion: @escaping (Data?) -> Void) {
        FeedProbe(limit: maximumFeedBytes).run(url, timeout: 6, completion: completion)
    }

    private static func interval(for data: Data) -> TimeInterval {
        data.count <= 4096 ? 10 : 20
    }

    private static func jsonObject(in data: Data) -> [String: Any]? {
        guard let any = try? JSONSerialization.jsonObject(with: data) else { return nil }
        return any as? [String: Any]
    }

    private static func clean(_ value: String?) -> String? {
        let trimmed = (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func join(_ artist: String?, _ title: String?) -> String? {
        let artist = (artist ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let title = (title ?? "").trimmingCharacters(in: .whitespacesAndNewlines)

        switch (artist.isEmpty, title.isEmpty) {
        case (false, false):
            return artist == title ? title : "\(artist) — \(title)"
        case (false, true):
            return artist
        case (true, false):
            return title
        case (true, true):
            return nil
        }
    }
}

/// One bounded fetch: stops reading as soon as the document is too big to be
/// worth polling, so a 66 KB server status never gets downloaded in full.
private final class FeedProbe: NSObject, URLSessionDataDelegate {
    private let limit: Int
    private var completion: ((Data?) -> Void)?
    private var buffer = Data()
    private var session: URLSession?
    private var task: URLSessionDataTask?
    private var finished = false

    init(limit: Int) {
        self.limit = limit
        super.init()
    }

    func run(_ url: URL, timeout: TimeInterval, completion: @escaping (Data?) -> Void) {
        self.completion = completion

        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = timeout
        configuration.timeoutIntervalForResource = timeout
        let session = URLSession(configuration: configuration, delegate: self, delegateQueue: nil)
        self.session = session

        var request = URLRequest(url: url)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("Radio/1.0 (macOS)", forHTTPHeaderField: "User-Agent")

        let task = session.dataTask(with: request)
        self.task = task
        task.resume()

        DispatchQueue.main.asyncAfter(deadline: .now() + timeout + 0.5) { [weak self] in
            self?.finish(nil)
        }
    }

    func urlSession(_ session: URLSession, dataTask: URLSessionDataTask, didReceive data: Data) {
        guard !finished else { return }
        buffer.append(data)
        if buffer.count > limit {
            finish(nil)
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        finish(error == nil ? buffer : nil)
    }

    private func finish(_ result: Data?) {
        guard !finished else { return }
        finished = true
        task?.cancel()
        task = nil
        session?.invalidateAndCancel()
        session = nil

        let completion = self.completion
        self.completion = nil
        completion?(result)
    }
}
