import Foundation

/// Many stations hand out `.m3u` or `.pls` playlists instead of a stream URL.
/// Those files are small text documents; fetch one and pull out the first
/// playable stream address.
enum PlaylistResolver {
    enum ResolutionError: LocalizedError {
        case fetchFailed(String)
        case noStreamInPlaylist

        var errorDescription: String? {
            switch self {
            case .fetchFailed(let reason):
                return reason
            case .noStreamInPlaylist:
                return "The playlist didn’t contain a stream URL."
            }
        }
    }

    static func isPlaylist(_ url: URL) -> Bool {
        switch url.pathExtension.lowercased() {
        case "m3u", "pls":
            return true
        default:
            return false
        }
    }

    static func resolve(_ url: URL, completion: @escaping (Result<URL, ResolutionError>) -> Void) {
        var request = URLRequest(url: url)
        request.timeoutInterval = 10
        request.setValue("Radio/1.0 (macOS)", forHTTPHeaderField: "User-Agent")

        URLSession.shared.dataTask(with: request) { data, response, error in
            let result = outcome(url: url, data: data, response: response, error: error)
            DispatchQueue.main.async {
                completion(result)
            }
        }.resume()
    }

    private static func outcome(
        url: URL,
        data: Data?,
        response: URLResponse?,
        error: Error?
    ) -> Result<URL, ResolutionError> {
        if let error {
            return .failure(.fetchFailed(error.localizedDescription))
        }
        if let status = (response as? HTTPURLResponse)?.statusCode, !(200..<300).contains(status) {
            return .failure(.fetchFailed("The playlist returned HTTP \(status)."))
        }
        guard
            let data,
            let text = String(data: data, encoding: .utf8) ?? String(data: data, encoding: .isoLatin1),
            let stream = streamURL(in: text, relativeTo: url)
        else {
            return .failure(.noStreamInPlaylist)
        }
        return .success(stream)
    }

    private static func streamURL(in text: String, relativeTo base: URL) -> URL? {
        let lines = text.split(whereSeparator: \.isNewline).map {
            $0.trimmingCharacters(in: .whitespaces)
        }

        // An HLS playlist served from an .m3u path: let AVPlayer read it directly.
        if lines.contains(where: { $0.hasPrefix("#EXT-X-") }) {
            return base
        }

        for line in lines {
            guard
                let candidate = streamCandidate(in: line),
                let url = URL(string: candidate, relativeTo: base)?.absoluteURL,
                Station.isPlayable(url)
            else { continue }
            return url
        }

        return nil
    }

    /// The stream address on a playlist line, or nil for comments, section
    /// headers and non-stream metadata.
    private static func streamCandidate(in line: String) -> String? {
        if line.isEmpty || line.hasPrefix("#") || line.hasPrefix("[") {
            return nil
        }

        let lowered = line.lowercased()
        if lowered.hasPrefix("http://") || lowered.hasPrefix("https://") {
            return line
        }

        if let equals = line.firstIndex(of: "=") {
            // PLS entries look like `File1=http://…`; other `Key=Value` lines go.
            guard line[..<equals].lowercased().hasPrefix("file") else { return nil }
            return line[line.index(after: equals)...].trimmingCharacters(in: .whitespaces)
        }

        return line // a relative reference, resolved against the playlist URL
    }
}
