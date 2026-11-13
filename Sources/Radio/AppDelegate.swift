import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let store = StationStore()
    private let player = RadioPlayer()
    private let menu = NSMenu()

    private var statusItem: NSStatusItem!
    private weak var headerItem: NSMenuItem?
    private weak var trackItem: NSMenuItem?
    private var isMenuOpen = false

    func applicationDidFinishLaunching(_ notification: Notification) {
        installMainMenu()

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        menu.delegate = self
        menu.autoenablesItems = false
        statusItem.menu = menu

        store.onChange = { [weak self] in self?.refreshStatusItem() }
        player.onChange = { [weak self] in self?.refreshStatusItem() }

        refreshStatusItem()
    }

    // MARK: - Status item

    private func refreshStatusItem() {
        guard let button = statusItem?.button else { return }
        button.image = Self.symbol(statusSymbol)
        button.toolTip = player.current.map { station in
            ["Radio", station.title, statusDescription, player.trackTitle]
                .compactMap { $0 }
                .joined(separator: " — ")
        } ?? "Radio"

        refreshOpenMenuItems()
    }

    /// The menu is built when it opens. Keep its status rows current while it
    /// stays open, without rebuilding it (that would disturb the slider drag).
    private func refreshOpenMenuItems() {
        guard isMenuOpen else { return }

        headerItem?.title = nowPlayingTitle

        guard let trackItem else { return }
        // Text only. Assigning `toolTip` to a menu item while its menu is open
        // makes AppKit try to show a tooltip panel mid-tracking, which throws
        // and aborts the app. The tooltip is refreshed when the menu is rebuilt.
        guard let track = player.trackTitle else {
            // The station moved on to news or an ad: clear the row rather than
            // leaving the previous song sitting there.
            trackItem.attributedTitle = NSAttributedString(string: "")
            return
        }
        trackItem.attributedTitle = Self.trackTitle(track)
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

    private static func trackTitle(_ track: String) -> NSAttributedString {
        let text = track.count > 60 ? String(track.prefix(59)) + "…" : track
        return NSAttributedString(
            string: "♪  \(text)",
            attributes: [
                .font: NSFont.systemFont(ofSize: 11),
                .foregroundColor: NSColor.secondaryLabelColor
            ]
        )
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

    func menuWillOpen(_ menu: NSMenu) {
        isMenuOpen = true
    }

    func menuDidClose(_ menu: NSMenu) {
        isMenuOpen = false
    }

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
        headerItem = header

        trackItem = nil
        if let track = player.trackTitle {
            let item = NSMenuItem(
                title: "",
                action: #selector(searchTrackOnYouTube),
                keyEquivalent: ""
            )
            item.target = self
            item.attributedTitle = Self.trackTitle(track)
            item.toolTip = "Search YouTube for “\(track)”"
            menu.addItem(item)
            trackItem = item
        }

        let toggle = NSMenuItem(
            title: player.isActive ? "Pause" : "Play",
            action: #selector(togglePlayback),
            keyEquivalent: ""
        )
        toggle.target = self
        toggle.isEnabled = player.current != nil
        toggle.image = Self.symbol(player.isActive ? "pause.fill" : "play.fill")
        menu.addItem(toggle)

        // The slider goes in last: AppKit only knows the menu's width once the
        // text rows are in it, and the slider is a fixed-width custom view.
        let sliderIndex = menu.numberOfItems
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

        let width = max(Self.minimumSliderWidth, menu.size.width)
        menu.insertItem(volumeItem(width: width), at: sliderIndex)
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

    private static let minimumSliderWidth: CGFloat = 240

    private func volumeItem(width: CGFloat) -> NSMenuItem {
        let item = NSMenuItem()
        let container = NSView(frame: NSRect(x: 0, y: 0, width: width, height: 26))

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
        slider.frame = NSRect(x: 38, y: 3, width: width - 46, height: 20)
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

    @objc private func searchTrackOnYouTube() {
        guard let track = player.trackTitle, let url = YouTubeSearch.url(for: track) else { return }
        NSWorkspace.shared.open(url)
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
