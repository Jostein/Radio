import AppKit

final class AddStationForm: NSObject, NSTextFieldDelegate {
    private let alert = NSAlert()
    private let titleField = NSTextField()
    private let urlField = NSTextField()
    private weak var addButton: NSButton?

    override init() {
        super.init()

        alert.messageText = "Add Station"
        alert.informativeText = "Paste a stream URL — direct links, .m3u8, .m3u and .pls playlists all work."
        alert.addButton(withTitle: "Add")
        alert.addButton(withTitle: "Cancel")
        addButton = alert.buttons.first

        titleField.frame = NSRect(x: 0, y: 36, width: 300, height: 24)
        titleField.placeholderString = "Title"
        titleField.delegate = self

        urlField.frame = NSRect(x: 0, y: 4, width: 300, height: 24)
        urlField.placeholderString = "Stream URL"
        urlField.delegate = self

        let accessory = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 64))
        accessory.addSubview(titleField)
        accessory.addSubview(urlField)
        alert.accessoryView = accessory
        alert.window.initialFirstResponder = titleField

        validate()
    }

    /// Shows the dialog and returns the new station, or nil if it was cancelled.
    func runModal() -> Station? {
        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        return Station(title: trimmed(titleField), urlString: trimmed(urlField))
    }

    func controlTextDidChange(_ notification: Notification) {
        validate()
    }

    private func validate() {
        addButton?.isEnabled = !trimmed(titleField).isEmpty
            && Station.streamURL(from: trimmed(urlField)) != nil
    }

    private func trimmed(_ field: NSTextField) -> String {
        field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
