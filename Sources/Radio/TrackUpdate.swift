import Foundation

/// The answer to one attempt at learning what is playing.
enum TrackUpdate {
    case title(String)
    /// The source answered, but has no song right now: news, ads, a jingle, or
    /// a station that publishes nothing at all.
    case noTitle
    /// The source could not be read this time: no metadata, a network error, or
    /// silence. The last known title stays on screen.
    case unavailable
}
