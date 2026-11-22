import Foundation
import SwiftUI

@MainActor
final class RadioModel: ObservableObject {
    @Published private(set) var stations: [Station] = []
    @Published private(set) var current: Station?
    @Published private(set) var isPlaying = false
    @Published private(set) var track: String?
    @Published private(set) var statusText = "Nothing playing"
    @Published var showingAdd = false

    private let store: StationStore
    private let player: Player
    private let systemNowPlaying = SystemNowPlaying()

    init() {
        let store = StationStore()
        let player = Player()
        self.store = store
        self.player = player
        stations = store.stations

        store.onChange = { [weak self] in
            self?.stations = store.stations
        }
        player.onChange = { [weak self] in
            self?.sync()
        }

        systemNowPlaying.install(player: player)
        sync()
    }

    func add(_ station: Station) {
        store.add(station)
        stations = store.stations
        play(station)
    }

    func remove(at offsets: IndexSet) {
        let removed = offsets.compactMap { index in
            stations.indices.contains(index) ? stations[index] : nil
        }
        for station in removed {
            if current?.id == station.id {
                player.stop()
            }
            store.remove(station)
        }
        stations = store.stations
        sync()
    }

    func select(_ station: Station) {
        if current?.id == station.id {
            toggle()
        } else {
            play(station)
        }
    }

    func play(_ station: Station) {
        player.play(station)
        sync()
    }

    func toggle() {
        guard current != nil else { return }
        player.togglePlayback()
        sync()
    }

    private func sync() {
        current = player.current
        isPlaying = player.isActive
        track = player.trackTitle
        statusText = player.statusDescription

        systemNowPlaying.update(
            station: current?.title,
            track: track,
            isPlaying: isPlaying
        )
    }
}
