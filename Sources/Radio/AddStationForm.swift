import AppKit

final class AddStationForm: NSObject, NSTextFieldDelegate {
    private let alert = NSAlert()
    private let urlField = NSTextField()
    private let titleField = NSTextField()
    private weak var addButton: NSButton?

    private let fieldWidth: CGFloat = 420

    private var suggestionWorkItem: DispatchWorkItem?
    private var suggestionToken = 0
    private var suggestedTitle: String?

    override init() {
        super.init()

        alert.messageText = "Add Station"
        alert.informativeText = "Paste a stream URL. The title is filled in when the station announces one — edit it if you like."
        alert.addButton(withTitle: "Add")
        alert.addButton(withTitle: "Cancel")
        addButton = alert.buttons.first

        // URL first: it is what you actually have to paste.
        urlField.frame = NSRect(x: 0, y: 38, width: fieldWidth, height: 58)
        urlField.placeholderString = "Stream URL"
        urlField.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        urlField.delegate = self
        urlField.usesSingleLineMode = false
        urlField.maximumNumberOfLines = 3
        urlField.lineBreakMode = .byCharWrapping
        urlField.cell?.wraps = true
        urlField.cell?.isScrollable = false

        titleField.frame = NSRect(x: 0, y: 4, width: fieldWidth, height: 24)
        titleField.placeholderString = "Title"
        titleField.delegate = self

        let accessory = NSView(frame: NSRect(x: 0, y: 0, width: fieldWidth, height: 96))
        accessory.addSubview(urlField)
        accessory.addSubview(titleField)
        alert.accessoryView = accessory
        alert.window.initialFirstResponder = urlField

        validate()
    }

    /// Shows the dialog and returns the new station, or nil if it was cancelled.
    func runModal() -> Station? {
        guard alert.runModal() == .alertFirstButtonReturn, let url = streamURL else { return nil }
        return Station(title: trimmed(titleField), urlString: url.absoluteString)
    }

    // MARK: - Validation and suggestions

    func controlTextDidChange(_ notification: Notification) {
        validate()
        if (notification.object as? NSTextField) === urlField {
            scheduleTitleSuggestion()
        }
    }

    /// Return submits, or moves on to the title when the URL is all that is set.
    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        guard control === urlField, commandSelector == #selector(NSResponder.insertNewline(_:)) else {
            return false
        }
        if addButton?.isEnabled == true {
            addButton?.performClick(nil)
        } else if streamURL != nil {
            alert.window.makeFirstResponder(titleField)
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

    // MARK: - Title detection

    private func scheduleTitleSuggestion() {
        suggestionWorkItem?.cancel()
        suggestionWorkItem = nil
        suggestionToken += 1

        guard let url = streamURL else { return }
        let token = suggestionToken

        let work = DispatchWorkItem { [weak self] in
            StreamMetadata.suggestedTitle(for: url) { [weak self] title in
                guard let self, token == self.suggestionToken else { return }
                self.applySuggestion(title)
            }
        }
        suggestionWorkItem = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4, execute: work)
    }

    private func applySuggestion(_ title: String?) {
        let current = trimmed(titleField)

        // Never overwrite something the user typed themselves.
        guard current.isEmpty || current == suggestedTitle else { return }

        if let title, !title.isEmpty {
            suggestedTitle = title
            titleField.stringValue = title
        } else if current == suggestedTitle {
            suggestedTitle = nil
            titleField.stringValue = ""
        }

        validate()
    }
}
