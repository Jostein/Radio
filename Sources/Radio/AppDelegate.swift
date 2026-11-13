import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let store = StationStore()
    private let player = RadioPlayer()
    private let menu = NSMenu()

    private var statusItem: NSStatusItem!

    func applicationDidFinishLaunching(_ notification: Notification) {
        installMainMenu()

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        menu.delegate = self
        menu.autoenablesItems = false
        statusItem.menu = menu

        store.onChange = { [weak self] in self?.refreshStatusItem() }
        player.onChange = { [weak self] in self?.refreshStatusItem() }

        refreshStatusItem()
        rebuildMenu()
    }

    // MARK: - Status item

    private func refreshStatusItem() {
        guard let button = statusItem?.button else { return }
        button.image = Self.symbol(statusSymbol)
        button.toolTip = player.current.map {
            "Radio — \($0.title) — \(statusDescription)"
        } ?? "Radio"
    }

    private var statusSymbol: String {
        switch player.state {
        case .connecting, .playing:
            return "waveform"
        case .reconnecting:
            return "arrow.triangle.2.circlepath"
        case .idle, .paused, .failed:
            return "dot.radiowaves.left.and.right"
        }
    }

    private static func symbol(_ name: String) -> NSImage? {
        let image = NSImage(systemSymbolName: name, accessibilityDescription: nil)
        image?.isTemplate = true
        return image
    }

    // MARK: - Main menu

    /// A menu bar app never shows its main menu, but it is what dispatches the
    /// standard key equivalents (⌘C, ⌘V, ⌘X, ⌘A, ⌘Z) to the text fields in the
    /// add-station dialog. Without it those shortcuts do nothing.
    private func installMainMenu() {
        let mainMenu = NSMenu()

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(
            withTitle: "Quit Radio",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        )
        appItem.submenu = appMenu
        mainMenu.addItem(appItem)

        let editItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Undo", action: NSSelectorFromString("undo:"), keyEquivalent: "z")
        editMenu.addItem(withTitle: "Redo", action: NSSelectorFromString("redo:"), keyEquivalent: "Z")
        editMenu.addItem(.separator())
        editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = editMenu
        mainMenu.addItem(editItem)

        NSApp.mainMenu = mainMenu
    }

    // MARK: - Menu

    func menuNeedsUpdate(_ menu: NSMenu) {
        rebuildMenu()
    }

    private var statusDescription: String {
        switch player.state {
        case .idle:
            return "Nothing playing"
        case .connecting:
            return "Connecting…"
        case .playing:
            return "Live"
        case .paused:
            return "Paused"
        case .reconnecting(let attempt):
            return "Reconnecting (\(attempt) of \(RadioPlayer.maxReconnectAttempts))…"
        case .failed(let message):
            return message
        }
    }

    private var nowPlayingTitle: String {
        guard let station = player.current else { return statusDescription }
        if case .failed(let message) = player.state {
            return "⚠︎ \(station.title): \(message)"
        }
        return "\(station.title) — \(statusDescription)"
    }

    private func rebuildMenu() {
        menu.removeAllItems()

        let header = NSMenuItem(title: nowPlayingTitle, action: nil, keyEquivalent: "")
        header.isEnabled = false
        menu.addItem(header)

        let toggle = NSMenuItem(
            title: player.isActive ? "Pause" : "Play",
            action: #selector(togglePlayback),
            keyEquivalent: ""
        )
        toggle.target = self
        toggle.isEnabled = player.current != nil
        toggle.image = Self.symbol(player.isActive ? "pause.fill" : "play.fill")
        menu.addItem(toggle)

        menu.addItem(volumeItem())
        menu.addItem(.separator())

        if store.stations.isEmpty {
            let empty = NSMenuItem(title: "No stations yet", action: nil, keyEquivalent: "")
            empty.isEnabled = false
            menu.addItem(empty)
        } else {
            for station in store.stations {
                let item = NSMenuItem(title: station.title, action: #selector(playStation(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = station.id
                item.state = player.current?.id == station.id ? .on : .off
                item.toolTip = station.urlString
                menu.addItem(item)
            }
        }

        menu.addItem(.separator())

        let add = NSMenuItem(title: "Add Station…", action: #selector(addStation), keyEquivalent: "n")
        add.target = self
        add.image = Self.symbol("plus")
        menu.addItem(add)

        if !store.stations.isEmpty {
            let removeRoot = NSMenuItem(title: "Remove Station", action: nil, keyEquivalent: "")
            removeRoot.submenu = removalSubmenu()
            menu.addItem(removeRoot)
        }

        menu.addItem(.separator())

        let quit = NSMenuItem(title: "Quit Radio", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        quit.image = Self.symbol("power")
        menu.addItem(quit)
    }

    private func removalSubmenu() -> NSMenu {
        let submenu = NSMenu()
        for station in store.stations {
            let item = NSMenuItem(title: station.title, action: #selector(removeStation(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = station.id
            submenu.addItem(item)
        }
        return submenu
    }

    private func volumeItem() -> NSMenuItem {
        let item = NSMenuItem()
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 230, height: 26))

        let speaker = NSImageView(frame: NSRect(x: 14, y: 5, width: 16, height: 16))
        speaker.image = NSImage(systemSymbolName: "speaker.fill", accessibilityDescription: "Volume")
        speaker.contentTintColor = .secondaryLabelColor
        speaker.imageScaling = .scaleProportionallyUpOrDown

        let slider = NSSlider(
            value: player.volume,
            minValue: 0,
            maxValue: 1,
            target: self,
            action: #selector(volumeChanged(_:))
        )
        slider.frame = NSRect(x: 38, y: 3, width: 180, height: 20)
        slider.controlSize = .small
        slider.isContinuous = true

        container.addSubview(speaker)
        container.addSubview(slider)
        item.view = container
        return item
    }

    // MARK: - Actions

    @objc private func togglePlayback() {
        player.togglePlayback()
    }

    @objc private func volumeChanged(_ sender: NSSlider) {
        player.volume = sender.doubleValue
    }

    @objc private func playStation(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? UUID, let station = store.station(with: id) else { return }
        if player.current?.id == station.id {
            player.togglePlayback()
        } else {
            player.play(station)
        }
    }

    @objc private func removeStation(_ sender: NSMenuItem) {
        guard let id = sender.representedObject as? UUID, let station = store.station(with: id) else { return }
        if player.current?.id == station.id {
            player.stop()
        }
        store.remove(station)
    }

    @objc private func addStation() {
        NSApp.activate(ignoringOtherApps: true)
        let form = AddStationForm()
        guard let station = form.runModal() else { return }
        store.add(station)
    }
}
