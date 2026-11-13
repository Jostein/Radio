import AppKit

/// The dimmed "♪  …" row of the status menu.
///
/// Titles come from the station and can be any length. Once the menu is open its
/// width must not change — a longer title would resize the window under the
/// pointer and leave the volume row looking short — so live updates are fitted
/// to the width the menu was built with.
enum TrackLabel {
    static let prefix = "♪  "
    private static let font = NSFont.systemFont(ofSize: 11)
    private static let maximumCharacters = 60

    static func attributed(_ track: String, width: CGFloat = .infinity) -> NSAttributedString {
        let capped = track.count > maximumCharacters
            ? String(track.prefix(maximumCharacters - 1)) + "…"
            : track

        return NSAttributedString(
            string: prefix + fitted(capped, width: width),
            attributes: [
                .font: font,
                .foregroundColor: NSColor.secondaryLabelColor
            ]
        )
    }

    static func width(of text: String) -> CGFloat {
        text.size(withAttributes: [.font: font]).width
    }

    /// Shortens `text` until the whole row, music note included, fits `limit`.
    private static func fitted(_ text: String, width limit: CGFloat) -> String {
        guard limit.isFinite, width(of: prefix + text) > limit else { return text }

        var trimmed = text
        while !trimmed.isEmpty, width(of: prefix + trimmed + "…") > limit {
            trimmed.removeLast()
        }
        return trimmed.isEmpty ? "" : trimmed + "…"
    }
}
