import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: RadioModel

    var body: some View {
        NavigationStack {
            List {
                Section {
                    NowPlayingCard()
                        .listRowInsets(EdgeInsets(top: 8, leading: 16, bottom: 8, trailing: 16))
                }

                Section("Stations") {
                    if model.stations.isEmpty {
                        Text("No stations yet. Add a stream to begin.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(model.stations) { station in
                            Button {
                                model.select(station)
                            } label: {
                                StationRow(
                                    station: station,
                                    isCurrent: model.current?.id == station.id,
                                    isPlaying: model.current?.id == station.id && model.isPlaying
                                )
                            }
                            .buttonStyle(.plain)
                        }
                        .onDelete(perform: model.remove)
                    }
                }

                Section {
                    Button {
                        model.showingAdd = true
                    } label: {
                        Label("Add Station", systemImage: "plus.circle.fill")
                    }
                }
            }
            .navigationTitle("Radio")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.showingAdd = true
                    } label: {
                        Image(systemName: "plus")
                    }
                    .accessibilityLabel("Add station")
                }
            }
            .sheet(isPresented: $model.showingAdd) {
                NavigationStack {
                    AddStationView { station in
                        model.add(station)
                    }
                }
            }
        }
        .tint(.blue)
    }
}

private struct NowPlayingCard: View {
    @EnvironmentObject private var model: RadioModel

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: model.isPlaying ? "waveform" : "dot.radiowaves.left.and.right")
                    .font(.title2)
                    .foregroundStyle(.blue)
                    .frame(width: 30)

                VStack(alignment: .leading, spacing: 3) {
                    Text(model.current?.title ?? "Choose a station")
                        .font(.headline)
                    Text(model.track ?? model.current.map { _ in model.statusText } ?? "Live radio, one stream at a time.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }

                Spacer(minLength: 0)
            }

            HStack(spacing: 12) {
                Button {
                    model.toggle()
                } label: {
                    Label(model.isPlaying ? "Pause" : "Play",
                          systemImage: model.isPlaying ? "pause.fill" : "play.fill")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(model.current == nil)

                Text(model.statusText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 4)
    }
}

private struct StationRow: View {
    let station: Station
    let isCurrent: Bool
    let isPlaying: Bool

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: isCurrent ? (isPlaying ? "waveform" : "pause.circle.fill") : "dot.radiowaves.left.and.right")
                .foregroundStyle(isCurrent ? .blue : .secondary)
                .frame(width: 24)

            VStack(alignment: .leading, spacing: 2) {
                Text(station.title)
                    .foregroundStyle(.primary)
                if isCurrent {
                    Text(isPlaying ? "Playing" : "Paused")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer()

            if isCurrent {
                Image(systemName: "checkmark.circle.fill")
                    .foregroundStyle(.blue)
            }
        }
        .contentShape(Rectangle())
    }
}
