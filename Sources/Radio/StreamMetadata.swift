import Foundation

/// Best-effort station name for a stream URL.
///
/// Icecast/Shoutcast streams advertise `icy-name` in their response headers, and
/// `.pls`/`.m3u` playlists usually carry a title of their own. When the server
/// answers but names nothing, the URL path becomes a readable fallback.
enum StreamMetadata {
    static func suggestedTitle(for url: URL, completion: @escaping (String?) -> Void) {
        if PlaylistResolver.isPlaylist(url) {
            playlistMetadata(for: url) { title, reachable in
                completion(title ?? (reachable ? fallbackTitle(for: url) : nil))
            }
            return
        }

        ICYProbe().run(url: url) { name, reachable in
            completion(name ?? (reachable ? fallbackTitle(for: url) : nil))
        }
    }

    // MARK: - Playlists

    private static func playlistMetadata(for url: URL, completion: @escaping (String?, Bool) -> Void) {
        var request = URLRequest(url: url)
        request.timeoutInterval = 10
        request.setValue("Radio/1.0 (macOS)", forHTTPHeaderField: "User-Agent")

        URLSession.shared.dataTask(with: request) { data, response, error in
            let status = (response as? HTTPURLResponse)?.statusCode
            let reachable = error == nil && (status.map { (200..<300).contains($0) } ?? false)

            var detected: String?
            if reachable,
               let data,
               let text = String(data: data, encoding: .utf8) ?? String(data: data, encoding: .isoLatin1) {
                detected = title(inPlaylistText: text)
            }

            DispatchQueue.main.async {
                completion(detected, reachable)
            }
        }.resume()
    }

    private static func title(inPlaylistText text: String) -> String? {
        for line in text.split(whereSeparator: \.isNewline).map({ $0.trimmingCharacters(in: .whitespaces) }) {
            let lowered = line.lowercased()

            // M3U: #EXTINF:-1,Station Name
            if lowered.hasPrefix("#extinf"), let comma = line.lastIndex(of: ",") {
                let title = line[line.index(after: comma)...].trimmingCharacters(in: .whitespaces)
                if !title.isEmpty { return title }
            }

            // PLS: Title1=Station Name
            if lowered.hasPrefix("title"), let equals = line.firstIndex(of: "=") {
                let title = line[line.index(after: equals)...].trimmingCharacters(in: .whitespaces)
                if !title.isEmpty { return title }
            }
        }
        return nil
    }

    // MARK: - URL fallback

    private static func fallbackTitle(for url: URL) -> String? {
        let generic: Set<String> = [
            "stream", "streams", "live", "listen", "audio", "radio", "index",
            "playlist", "default", "mp3", "aac", "hls", "m3u8", "m3u", "pls"
        ]

        var components = url.pathComponents.filter { $0 != "/" }
        if let last = components.popLast() {
            components.append((last as NSString).deletingPathExtension)
        }

        for raw in components.reversed() {
            let cleaned = raw
                .replacingOccurrences(of: "_", with: " ")
                .replacingOccurrences(of: "-", with: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)

            guard
                cleaned.count >= 3,
                cleaned.rangeOfCharacter(from: .letters) != nil,
                !generic.contains(cleaned.lowercased())
            else { continue }

            return cleaned.capitalized
        }

        return nil
    }
}

/// Asks a stream for its ICY headers and hangs up before any audio arrives.
private final class ICYProbe: NSObject, URLSessionDataDelegate {
    private var completion: ((String?, Bool) -> Void)?
    private var finished = false

    private lazy var session = URLSession(
        configuration: .ephemeral,
        delegate: self,
        delegateQueue: nil
    )

    func run(url: URL, completion: @escaping (String?, Bool) -> Void) {
        self.completion = completion

        var request = URLRequest(url: url)
        request.timeoutInterval = 8
        request.setValue("1", forHTTPHeaderField: "Icy-MetaData")
        request.setValue("Radio/1.0 (macOS)", forHTTPHeaderField: "User-Agent")

        session.dataTask(with: request).resume()

        // Never leave the dialog waiting on a stream that accepts connections
        // but never sends headers.
        DispatchQueue.main.asyncAfter(deadline: .now() + 9) { [weak self] in
            self?.finish(with: nil, reachable: false)
        }
    }

    func urlSession(
        _ session: URLSession,
        dataTask: URLSessionDataTask,
        didReceive response: URLResponse,
        completionHandler: @escaping (URLSession.ResponseDisposition) -> Void
    ) {
        let http = response as? HTTPURLResponse
        let status = http?.statusCode ?? 0
        let name = http?.value(forHTTPHeaderField: "icy-name")

        completionHandler(.cancel) // headers are all we need
        finish(with: name, reachable: (200..<300).contains(status))
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        finish(with: nil, reachable: false)
    }

    private func finish(with title: String?, reachable: Bool) {
        guard !finished else { return }
        finished = true
        session.invalidateAndCancel()

        let name = title?.trimmingCharacters(in: .whitespacesAndNewlines)
        let value = name?.isEmpty == false ? name : nil
        let completion = self.completion
        self.completion = nil

        // This arrives on a URLSession queue; the caller fills in a text field.
        let deliver: () -> Void = { completion?(value, reachable) }
        if Thread.isMainThread {
            deliver()
        } else {
            DispatchQueue.main.async(execute: deliver)
        }
    }
}
