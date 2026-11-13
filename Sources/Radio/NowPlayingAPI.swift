import Foundation

/// Stations that publish the current track as a small JSON document.
///
/// Polling one of these costs a few hundred bytes to a few kilobytes instead of
/// the `icy-metaint` block of audio, so it is both cheaper and faster. Only
/// broadcasters with a known, stable endpoint are listed; everything else keeps
/// using the stream's own metadata.
enum NowPlayingAPI {
    struct Source {
        let url: URL
        /// Bigger documents get a slower cadence; they still arrive sooner than
        /// a 30s audio-block poll would.
        let refreshInterval: TimeInterval
        let parse: (Data) -> String?
    }

    static func source(for streamURL: URL) -> Source? {
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
    private static func somaFM(_ streamURL: URL) -> Source? {
        guard let mount = streamURL.pathComponents.dropFirst().first else { return nil }
        let channel = mount.replacingOccurrences(
            of: "-[0-9]+-(mp3|aac[a-z0-9-]*)$",
            with: "",
            options: [.regularExpression, .caseInsensitive]
        )
        guard !channel.isEmpty, let url = URL(string: "https://somafm.com/songs/\(channel).json") else {
            return nil
        }

        return Source(url: url, refreshInterval: 10) { data in
            guard
                let root = jsonObject(in: data),
                let songs = root["songs"] as? [[String: Any]],
                let current = songs.first
            else { return nil }
            return join(current["artist"] as? String, current["title"] as? String)
        }
    }

    // MARK: - Radio Paradise

    private static func radioParadise(_ streamURL: URL) -> Source? {
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

        return Source(url: url, refreshInterval: 10) { data in
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

    private static func radioFrance(_ streamURL: URL) -> Source? {
        let address = streamURL.absoluteString.lowercased()
        let id = fipStations.first { address.contains($0.slug) }?.id ?? 7
        guard let url = URL(string: "https://api.radiofrance.fr/livemeta/pull/\(id)") else {
            return nil
        }

        // Radio France spends a few kilobytes on the schedule around the song,
        // so this one is polled less often and still beats the audio-block read.
        return Source(url: url, refreshInterval: 20) { data in
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

    // MARK: - Helpers

    private static func jsonObject(in data: Data) -> [String: Any]? {
        guard let any = try? JSONSerialization.jsonObject(with: data) else { return nil }
        return any as? [String: Any]
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
