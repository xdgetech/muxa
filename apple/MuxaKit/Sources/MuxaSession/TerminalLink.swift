import Foundation

/// A link the user opens from a terminal: an OSC 8 link or a URL in the text.
/// The text comes from the host, so this device opens only web and mail links,
/// never a file, an app or another scheme.
public enum TerminalLink {
    public static let schemes: Set<String> = ["http", "https", "mailto"]

    /// The URL to open, or nil when the link must not open on this device.
    public static func url(_ link: String) -> URL? {
        let text = link.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: text), let scheme = url.scheme?.lowercased(), schemes.contains(scheme)
        else { return nil }
        if scheme != "mailto", url.host?.isEmpty ?? true { return nil }
        return url
    }
}
