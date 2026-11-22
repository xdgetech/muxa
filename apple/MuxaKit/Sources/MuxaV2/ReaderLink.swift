import Foundation

/// Links between the Reader's documents (plan F2). A relative link in a
/// document names another one by its path from that document's folder.
public enum ReaderLink {
    /// The listed path a link in the document at `from` names, or nil: a web
    /// link, an absolute path, a path out of the folder, or a file not listed.
    public static func document(_ href: String, from: String, in entries: [ReaderEntry]) -> String? {
        guard let path = resolve(href, from: from) else { return nil }
        return entries.contains { $0.path == path } ? path : nil
    }

    /// `href` against the folder of `from`, without its fragment or query.
    static func resolve(_ href: String, from: String) -> String? {
        let bare = String(href.prefix { $0 != "#" && $0 != "?" })
        guard !bare.isEmpty, !bare.hasPrefix("/"), !hasScheme(bare),
              let decoded = bare.removingPercentEncoding else { return nil }
        var parts = from.split(separator: "/", omittingEmptySubsequences: true).dropLast().map(String.init)
        for part in decoded.split(separator: "/", omittingEmptySubsequences: true) {
            switch part {
            case ".": continue
            case "..":
                guard !parts.isEmpty else { return nil }
                parts.removeLast()
            default: parts.append(String(part))
            }
        }
        return parts.isEmpty ? nil : parts.joined(separator: "/")
    }

    /// `http:`, `mailto:`, `javascript:`…: a colon before the first slash.
    private static func hasScheme(_ s: String) -> Bool {
        guard let colon = s.firstIndex(of: ":") else { return false }
        return s.firstIndex(of: "/").map { colon < $0 } ?? true
    }
}
