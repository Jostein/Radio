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
        // Pasting can carry stray whitespace or line breaks; URLs can't contain them.
        let cleaned = urlString.filter { !$0.isWhitespace }
        guard let url = URL(string: cleaned), isPlayable(url) else { return nil }
        return url
    }

    static func isPlayable(_ url: URL) -> Bool {
        guard let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else {
            return false
        }
        return url.host != nil
    }
}
