import Foundation

final class StationStore {
    private(set) var stations: [Station] = []

    var onChange: (() -> Void)?

    private let fileURL: URL

    private let starterStations = [
        Station(title: "FIP Reggae", urlString: "https://stream.radiofrance.fr/fipreggae/fipreggae.m3u8")
    ]

    init() {
        let support = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Library/Application Support", isDirectory: true)
        let directory = support.appendingPathComponent("Radio", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        fileURL = directory.appendingPathComponent("stations.json")
        load()
    }

    func add(_ station: Station) {
        stations.append(station)
        save()
        onChange?()
    }

    func remove(_ station: Station) {
        stations.removeAll { $0.id == station.id }
        save()
        onChange?()
    }

    func station(with id: UUID) -> Station? {
        stations.first { $0.id == id }
    }

    private func load() {
        guard
            let data = try? Data(contentsOf: fileURL),
            let saved = try? JSONDecoder().decode([Station].self, from: data)
        else {
            stations = starterStations
            return
        }
        stations = saved
    }

    private func save() {
        guard let data = try? JSONEncoder().encode(stations) else { return }
        try? data.write(to: fileURL, options: .atomic)
    }
}
