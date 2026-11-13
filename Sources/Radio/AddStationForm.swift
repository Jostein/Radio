import AppKit

final class AddStationForm: NSObject, NSTextFieldDelegate {
    private let alert = NSAlert()
    private let titleField = NSTextField()
    private let urlField = NSTextField()
    private weak var addButton: NSButton?

    private let fieldWidth: CGFloat = 420

    override init() {
        super.init()

        alert.messageText = "Add Station"
        alert.informativeText = "Paste a stream URL — direct links, .m3u8, .m3u and .pls playlists all work."
        alert.addButton(withTitle: "Add")
        alert.addButton(withTitle: "Cancel")
        addButton = alert.buttons.first

        titleField.frame = NSRect(x: 0, y: 72, width: fieldWidth, height: 24)
        titleField.placeholderString = "Title"
        titleField.delegate = self

        // Stream URLs get long, so this field wraps over up to three lines
        // instead of scrolling the interesting part out of sight.
        urlField.frame = NSRect(x: 0, y: 4, width: fieldWidth, height: 58)
        urlField.placeholderString = "Stream URL"
        urlField.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        urlField.delegate = self
        urlField.usesSingleLineMode = false
        urlField.maximumNumberOfLines = 3
        urlField.lineBreakMode = .byCharWrapping
        urlField.cell?.wraps = true
        urlField.cell?.isScrollable = false

        let accessory = NSView(frame: NSRect(x: 0, y: 0, width: fieldWidth, height: 100))
        accessory.addSubview(titleField)
        accessory.addSubview(urlField)
        alert.accessoryView = accessory
        alert.window.initialFirstResponder = titleField

        validate()
    }

    /// Shows the dialog and returns the new station, or nil if it was cancelled.
    func runModal() -> Station? {
        guard alert.runModal() == .alertFirstButtonReturn, let url = streamURL else { return nil }
        return Station(title: trimmed(titleField), urlString: url.absoluteString)
    }

    // MARK: - Validation

    func controlTextDidChange(_ notification: Notification) {
        validate()
    }

    /// Return submits from the URL field rather than inserting a line break.
    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        guard control === urlField, commandSelector == #selector(NSResponder.insertNewline(_:)) else {
            return false
        }
        if addButton?.isEnabled == true {
            addButton?.performClick(nil)
        }
        return true
    }

    private var streamURL: URL? {
        Station.streamURL(from: urlField.stringValue)
    }

    private func validate() {
        addButton?.isEnabled = !trimmed(titleField).isEmpty && streamURL != nil
    }

    private func trimmed(_ field: NSTextField) -> String {
        field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
