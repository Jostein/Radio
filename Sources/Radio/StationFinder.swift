import AppKit

/// A small search panel, alive only while it is open.
///
/// It holds one array of results and one request in flight. Closing it releases
/// both; nothing is cached and nothing polls.
final class StationFinder: NSObject, NSSearchFieldDelegate, NSTableViewDataSource, NSTableViewDelegate, NSWindowDelegate {
    private let panel: NSPanel
    private let searchField = NSSearchField()
    private let table = NSTableView()
    private let statusLabel = NSTextField(labelWithString: "Type a name to search stations worldwide.")
    private let playButton = NSButton(title: "Play", target: nil, action: nil)

    private var results: [DirectoryStation] = []
    private var searchTask: Task<Void, Never>?
    private var pendingSearch: DispatchWorkItem?

    private let onPick: (Station) -> Void
    var onClose: (() -> Void)?

    init(onPick: @escaping (Station) -> Void) {
        self.onPick = onPick
        panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 440, height: 400),
            styleMask: [.titled, .closable, .utilityWindow],
            backing: .buffered,
            defer: false
        )
        super.init()

        panel.title = "Find a Station"
        panel.isFloatingPanel = true
        panel.hidesOnDeactivate = false
        panel.delegate = self

        buildContent()
    }

    // MARK: - Presentation

    func show() {
        NSApp.activate(ignoringOtherApps: true)
        panel.center()
        panel.makeKeyAndOrderFront(nil)
        panel.makeFirstResponder(searchField)
    }

    func windowWillClose(_ notification: Notification) {
        searchTask?.cancel()
        pendingSearch?.cancel()
        results = []
        onClose?()
        onClose = nil
    }

    // MARK: - Content

    private func buildContent() {
        let content = NSView()
        panel.contentView = content

        searchField.placeholderString = "Station name"
        searchField.delegate = self
        searchField.translatesAutoresizingMaskIntoConstraints = false

        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("station"))
        column.width = 390
        table.addTableColumn(column)
        table.headerView = nil
        table.rowHeight = 26
        table.allowsMultipleSelection = false
        table.dataSource = self
        table.delegate = self
        table.target = self
        table.doubleAction = #selector(pickSelected)

        let scroll = NSScrollView()
        scroll.documentView = table
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        scroll.translatesAutoresizingMaskIntoConstraints = false

        statusLabel.textColor = .secondaryLabelColor
        statusLabel.font = .systemFont(ofSize: 11)
        statusLabel.lineBreakMode = .byTruncatingTail
        statusLabel.translatesAutoresizingMaskIntoConstraints = false

        playButton.target = self
        playButton.action = #selector(pickSelected)
        playButton.keyEquivalent = "\r"
        playButton.isEnabled = false
        playButton.translatesAutoresizingMaskIntoConstraints = false

        let cancel = NSButton(title: "Cancel", target: self, action: #selector(cancel))
        cancel.keyEquivalent = "\u{1b}"
        cancel.translatesAutoresizingMaskIntoConstraints = false

        content.addSubview(searchField)
        content.addSubview(scroll)
        content.addSubview(statusLabel)
        content.addSubview(playButton)
        content.addSubview(cancel)

        NSLayoutConstraint.activate([
            searchField.topAnchor.constraint(equalTo: content.topAnchor, constant: 14),
            searchField.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 16),
            searchField.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -16),

            scroll.topAnchor.constraint(equalTo: searchField.bottomAnchor, constant: 10),
            scroll.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 16),
            scroll.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -16),
            scroll.bottomAnchor.constraint(equalTo: statusLabel.topAnchor, constant: -8),

            statusLabel.leadingAnchor.constraint(equalTo: content.leadingAnchor, constant: 16),
            statusLabel.trailingAnchor.constraint(equalTo: cancel.leadingAnchor, constant: -8),
            statusLabel.bottomAnchor.constraint(equalTo: content.bottomAnchor, constant: -14),

            cancel.trailingAnchor.constraint(equalTo: content.trailingAnchor, constant: -16),
            cancel.centerYAnchor.constraint(equalTo: statusLabel.centerYAnchor),

            playButton.trailingAnchor.constraint(equalTo: cancel.leadingAnchor, constant: -8),
            playButton.centerYAnchor.constraint(equalTo: statusLabel.centerYAnchor),
        ])
    }

    // MARK: - Searching

    func controlTextDidChange(_ notification: Notification) {
        pendingSearch?.cancel()

        let text = searchField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.count >= 2 else {
            searchTask?.cancel()
            results = []
            table.reloadData()
            playButton.isEnabled = false
            statusLabel.stringValue = text.isEmpty
                ? "Type a name to search stations worldwide."
                : "Keep typing…"
            return
        }

        let work = DispatchWorkItem { [weak self] in self?.search(text) }
        pendingSearch = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35, execute: work)
    }

    private func search(_ text: String) {
        searchTask?.cancel()
        statusLabel.stringValue = "Searching…"
        playButton.isEnabled = false

        searchTask = Task { @MainActor [weak self] in
            do {
                let found = try await StationDirectory.search(text)
                guard !Task.isCancelled, let self else { return }
                self.results = found
                self.table.reloadData()
                self.statusLabel.stringValue = found.isEmpty
                    ? "No stations found."
                    : "\(found.count) found — double-click to play."
                if !found.isEmpty {
                    self.table.selectRowIndexes(IndexSet(integer: 0), byExtendingSelection: false)
                    self.playButton.isEnabled = true
                }
            } catch is CancellationError {
                // A newer search replaced this one.
            } catch {
                guard let self else { return }
                self.statusLabel.stringValue = error.localizedDescription
            }
        }
    }

    // MARK: - Choosing

    @objc private func pickSelected() {
        let row = table.selectedRow
        guard results.indices.contains(row) else { return }
        let picked = results[row]
        guard let url = picked.streamURL else { return }
        onPick(Station(title: picked.name, urlString: url.absoluteString))
        panel.close()
    }

    @objc private func cancel() {
        panel.close()
    }

    // MARK: - Table

    func numberOfRows(in tableView: NSTableView) -> Int {
        results.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        let identifier = NSUserInterfaceItemIdentifier("station.cell")
        let cell: NSTableCellView
        if let reused = tableView.makeView(withIdentifier: identifier, owner: nil) as? NSTableCellView {
            cell = reused
        } else {
            cell = NSTableCellView()
            cell.identifier = identifier
            let label = NSTextField(labelWithString: "")
            label.translatesAutoresizingMaskIntoConstraints = false
            label.lineBreakMode = .byTruncatingTail
            cell.addSubview(label)
            cell.textField = label
            NSLayoutConstraint.activate([
                label.leadingAnchor.constraint(equalTo: cell.leadingAnchor, constant: 4),
                label.trailingAnchor.constraint(equalTo: cell.trailingAnchor, constant: -4),
                label.centerYAnchor.constraint(equalTo: cell.centerYAnchor),
            ])
        }

        let station = results[row]
        cell.textField?.stringValue = station.displayName
        cell.toolTip = station.detail
        return cell
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        playButton.isEnabled = results.indices.contains(table.selectedRow)
    }
}
