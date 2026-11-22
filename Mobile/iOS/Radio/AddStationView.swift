import SwiftUI

struct AddStationView: View {
    @Environment(\.dismiss) private var dismiss
    @FocusState private var focusedField: Field?

    @State private var title = ""
    @State private var urlString = ""

    let onAdd: (Station) -> Void

    private enum Field {
        case title
        case url
    }

    private var streamURL: URL? {
        Station.streamURL(from: urlString)
    }

    private var canAdd: Bool {
        streamURL != nil
    }

    var body: some View {
        Form {
            Section("Station") {
                TextField("Name", text: $title)
                    .textInputAutocapitalization(.words)
                    .focused($focusedField, equals: .title)

                TextField("https://example.com/stream.mp3", text: $urlString)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .focused($focusedField, equals: .url)
                    .onSubmit(add)
            }

            Section {
                Text("The name is what appears in your list. The URL may be an MP3, AAC, HLS, M3U or PLS stream.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Add Station")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Add", action: add)
                    .disabled(!canAdd)
            }
        }
        .onAppear {
            focusedField = .title
        }
    }

    private func add() {
        guard let streamURL else { return }

        let cleanTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
        let fallback = streamURL.host?.replacingOccurrences(of: "www.", with: "") ?? "Radio"
        onAdd(
            Station(
                title: cleanTitle.isEmpty ? fallback : cleanTitle,
                urlString: streamURL.absoluteString
            )
        )
        dismiss()
    }
}
