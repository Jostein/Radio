import Foundation

/// Finds a station's own "now playing" document without knowing the station.
///
/// Two public conventions exist: Icecast serves `/status-json.xsl` for the whole
/// server, and Shoutcast DNAS answers `/stats?json=1`. Both are probed once when
/// a stream starts. A feed is adopted only when it names a title for this very
/// mount and is small enough that polling it beats reading the audio stream's
/// inline ICY block — so discovery can improve things but never make them worse.
enum NowPlayingDiscovery {
    /// An ICY read costs 8–64 KB, so a bigger document would be a downgrade.
    private static let maximumFeedBytes = 16_384

    static func discover(for streamURL: URL, completion: @escaping (NowPlayingAPI.Source?) -> Void) {
        tryIcecast(streamURL) { source in
            if let source {
                completion(source)
                return
            }
            tryShoutcast(streamURL, completion: completion)
        }
    }

    // MARK: - Icecast

    private static func tryIcecast(_ streamURL: URL, completion: @escaping (NowPlayingAPI.Source?) -> Void) {
        guard let url = endpoint(for: streamURL, path: "/status-json.xsl") else {
            completion(nil)
            return
        }
        let mount = streamURL.lastPathComponent

        fetch(url) { data in
            guard let data, let title = icecastTitle(in: data, mount: mount) else {
                completion(nil)
                return
            }
            completion(
                NowPlayingAPI.Source(url: url, refreshInterval: interval(for: data)) { data in
                    icecastTitle(in: data, mount: mount)
                }
            )
        }
    }

    private static func icecastTitle(in data: Data, mount: String) -> String? {
        guard
            let any = try? JSONSerialization.jsonObject(with: data),
            let root = any as? [String: Any],
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

    // MARK: - Shoutcast

    private static func tryShoutcast(_ streamURL: URL, completion: @escaping (NowPlayingAPI.Source?) -> Void) {
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
                NowPlayingAPI.Source(url: url, refreshInterval: interval(for: data)) { data in
                    shoutcastTitle(in: data)
                }
            )
        }
    }

    private static func shoutcastTitle(in data: Data) -> String? {
        guard
            let any = try? JSONSerialization.jsonObject(with: data),
            let root = any as? [String: Any]
        else { return nil }

        for key in ["songtitle", "currentsong", "title"] {
            if let title = clean(root[key] as? String) {
                return title
            }
        }
        return nil
    }

    // MARK: - Plumbing

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

    private static func clean(_ value: String?) -> String? {
        let trimmed = (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
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
