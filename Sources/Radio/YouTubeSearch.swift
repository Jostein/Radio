import Foundation

enum YouTubeSearch {
    /// Search URL for a track title, or nil when there is nothing to search for.
    static func url(for query: String) -> URL? {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        var components = URLComponents(string: "https://www.youtube.com/results")
        components?.queryItems = [URLQueryItem(name: "search_query", value: trimmed)]
        return components?.url
    }
}
