import Foundation

struct Station: Codable {
    var id: UUID
    var title: String
    var urlString: String

    init(id: UUID = UUID(), title: String, urlString: String) {
        self.id = id
        self.title = title
        self.urlString = urlString
    }

    /// The playable stream URL, or nil when the stored string isn't an http(s) URL.
    var streamURL: URL? {
        Self.streamURL(from: urlString)
    }

    static func streamURL(from urlString: String) -> URL? {
        guard let url = URL(string: urlString), isPlayable(url) else { return nil }
        return url
    }

    static func isPlayable(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else {
            return false
        }
        return url.host != nil
    }
}
