import Foundation
import MuxaProtocol

/// Where a link in an item's summary leads (ADR 0033 §4a.5): the browser, or the
/// Reader at one of the item's attachments.
public enum SummaryLink: Equatable, Sendable {
    case web(URL)
    case doc(String)
    case image(String)

    private static let scheme = "muxa-evidence"

    /// The link as the summary's text carries it. An attachment gets an app-only URL.
    public var url: URL {
        switch self {
        case .web(let url): url
        case .doc(let path): Self.evidence("doc", path)
        case .image(let path): Self.evidence("image", path)
        }
    }

    /// The attachment's path, or the web link.
    public var path: String {
        switch self {
        case .web(let url): url.absoluteString
        case .doc(let path), .image(let path): path
        }
    }

    /// Nil for any scheme but `http`, `https` and the app's own.
    public init?(_ url: URL) {
        switch url.scheme?.lowercased() {
        case "http", "https":
            self = .web(url)
        case Self.scheme:
            let c = URLComponents(url: url, resolvingAgainstBaseURL: false)
            guard let path = c?.queryItems?.first(where: { $0.name == "path" })?.value else { return nil }
            switch c?.host {
            case "doc": self = .doc(path)
            case "image": self = .image(path)
            default: return nil
            }
        default:
            return nil
        }
    }

    private static func evidence(_ host: String, _ path: String) -> URL {
        var c = URLComponents()
        c.scheme = scheme
        c.host = host
        c.queryItems = [URLQueryItem(name: "path", value: path)]
        // URLComponents percent-encodes every part, so `url` is never nil here.
        return c.url ?? URL(fileURLWithPath: path)
    }
}

/// The item screen's summary: inline Markdown, with `http` and `https` links and
/// the file names of the item's docs and images as links. Other links are text.
public enum ItemSummary {
    public static func attributed(_ summary: String, attachments: Attachments) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        var text = (try? AttributedString(markdown: summary, options: options)) ?? AttributedString(summary)
        let marked = text.runs.compactMap { run in
            run.link.map { (run.range, SummaryLink($0) ?? attachment(named: $0.relativeString, in: attachments)) }
        }
        for (range, link) in marked { text[range].link = link?.url }
        let plain = String(text.characters)
        for (range, link) in webLinks(in: plain) + fileNames(in: plain, attachments: attachments) {
            guard let r = Range(range, in: text), text[r].runs.allSatisfy({ $0.link == nil }) else { continue }
            text[r].link = link.url
        }
        return text
    }

    /// Bare `http` and `https` URLs written with their scheme: the detector also
    /// takes `report.md` for a web address.
    private static func webLinks(in text: String) -> [(NSRange, SummaryLink)] {
        guard let detector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue) else { return [] }
        let ns = text as NSString
        return detector.matches(in: text, range: NSRange(location: 0, length: ns.length)).compactMap { match in
            let written = ns.substring(with: match.range).lowercased()
            guard written.hasPrefix("http://") || written.hasPrefix("https://"),
                let url = match.url, case .web? = SummaryLink(url)
            else { return nil }
            return (match.range, .web(url))
        }
    }

    /// Path-like words that name one of the attachments. A URL is not a file name.
    private static func fileNames(in text: String, attachments: Attachments) -> [(NSRange, SummaryLink)] {
        guard !attachments.docs.isEmpty || !attachments.images.isEmpty else { return [] }
        return text.matches(of: #/[^\s()\[\]<>"'`*,;]+/#).compactMap { match in
            var range = match.range
            while range.lowerBound < range.upperBound, ".:!?".contains(text[text.index(before: range.upperBound)]) {
                range = range.lowerBound..<text.index(before: range.upperBound)
            }
            let name = String(text[range])
            guard !name.contains("://"), let link = attachment(named: name, in: attachments) else { return nil }
            return (NSRange(range, in: text), link)
        }
    }

    /// The attachment `name` names: its path, a path ending in it (an absolute one),
    /// or a bare file name only one attachment has.
    static func attachment(named name: String, in a: Attachments) -> SummaryLink? {
        let name = name.hasPrefix("./") ? String(name.dropFirst(2)) : name
        guard !name.isEmpty else { return nil }
        let all = a.docs.map(SummaryLink.doc) + a.images.map(SummaryLink.image)
        if let exact = all.first(where: { $0.path == name || name.hasSuffix("/" + $0.path) }) { return exact }
        guard !name.contains("/") else { return nil }
        let named = all.filter { $0.path.split(separator: "/").last.map(String.init) == name }
        return named.count == 1 ? named[0] : nil
    }
}
