/// How much room a menu row gives one line before it gets truncated.
pub const MENU_TITLE_LIMIT: usize = 60;

/// Shortens a track title to `limit` characters, ending in an ellipsis.
///
/// The macOS app fits text to the measured menu width; a native menu row is
/// measured by the platform, so a character budget is the portable equivalent.
pub fn shorten(text: &str, limit: usize) -> String {
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= limit {
        return single_line;
    }

    let mut shortened: String = single_line.chars().take(limit.saturating_sub(1)).collect();
    shortened.push('…');
    shortened
}

/// The row text for a track: music note, then the shortened title.
pub fn row(track: &str) -> String {
    format!("♪  {}", shorten(track, MENU_TITLE_LIMIT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_short_titles() {
        assert_eq!(shorten("Madonna", 60), "Madonna");
    }

    #[test]
    fn collapses_line_breaks() {
        assert_eq!(shorten("Artist\n  Track", 60), "Artist Track");
    }

    #[test]
    fn truncates_with_an_ellipsis() {
        let long = "a".repeat(100);
        let shortened = shorten(&long, 20);
        assert_eq!(shortened.chars().count(), 20);
        assert!(shortened.ends_with('…'));
    }

    #[test]
    fn prefixes_the_row() {
        assert_eq!(row("Track"), "♪  Track");
    }
}
