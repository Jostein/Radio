import Foundation

/// One station as the public Radio Browser directory describes it.
struct DirectoryStation: Decodable, Identifiable {
    let id: String
    let name: String
    let country: String
    let codec: String
    let bitrate: Int
    let urlString: String

    var streamURL: URL? {
        Station.streamURL(from: urlString)
    }

    var displayName: String {
        country.isEmpty ? name : "\(name) — \(country)"
    }

    var detail: String {
        var parts: [String] = []
        if !codec.isEmpty { parts.append(codec) }
        if bitrate > 0 { parts.append("\(bitrate) kbps") }
        parts.append(urlString)
        return parts.joined(separator: " · ")
    }

    private enum CodingKeys: String, CodingKey {
        case id = "stationuuid"
        case name, country, codec, bitrate
        case resolved = "url_resolved"
        case url
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decodeIfPresent(String.self, forKey: .id) ?? UUID().uuidString
        name = (try container.decodeIfPresent(String.self, forKey: .name) ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        country = (try container.decodeIfPresent(String.self, forKey: .country) ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        codec = (try container.decodeIfPresent(String.self, forKey: .codec) ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        bitrate = try container.decodeIfPresent(Int.self, forKey: .bitrate) ?? 0

        let resolved = try container.decodeIfPresent(String.self, forKey: .resolved) ?? ""
        urlString = resolved.isEmpty
            ? (try container.decodeIfPresent(String.self, forKey: .url) ?? "")
            : resolved
    }
}

/// An on-demand query against radio-browser.info.
///
/// Nothing is fetched at launch and nothing is kept between searches: the answer
/// is held only while the search panel is open.
enum StationDirectory {
    private static let endpoint = URL(string: "https://de1.api.radio-browser.info/json/stations/search")!

    static func search(_ query: String, limit: Int = 30) async throws -> [DirectoryStation] {
        let text = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return [] }

        var components = URLComponents(url: endpoint, resolvingAgainstBaseURL: false)
        components?.queryItems = [
            URLQueryItem(name: "name", value: text),
            URLQueryItem(name: "limit", value: String(limit)),
            URLQueryItem(name: "hidebroken", value: "true"),
            URLQueryItem(name: "order", value: "clickcount"),
            URLQueryItem(name: "reverse", value: "true"),
        ]
        guard let url = components?.url else { throw DirectoryError.badResponse }

        var request = URLRequest(url: url)
        request.setValue("Radio/1.1 (macOS)", forHTTPHeaderField: "User-Agent")

        // A session per search: this one request, then nothing left holding
        // connections or a cache after the panel closes.
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 10
        configuration.timeoutIntervalForResource = 15
        configuration.urlCache = nil
        let session = URLSession(configuration: configuration)
        defer { session.finishTasksAndInvalidate() }

        let (data, response) = try await session.data(for: request)
        guard (response as? HTTPURLResponse)?.statusCode == 200 else {
            throw DirectoryError.badResponse
        }

        let stations = try JSONDecoder().decode([DirectoryStation].self, from: data)

        // Keep only playable entries, and only one copy of each stream.
        var seen = Set<String>()
        return stations.filter { station in
            guard !station.name.isEmpty, station.streamURL != nil else { return false }
            return seen.insert(station.urlString).inserted
        }
    }

    enum DirectoryError: LocalizedError {
        case badResponse

        var errorDescription: String? {
            "The station directory did not answer. Try again in a moment."
        }
    }
}
