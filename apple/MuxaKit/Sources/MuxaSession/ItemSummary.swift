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

/// The item screen's summary: block Markdown, with `http` and `https` links and the
/// file names of the item's docs and images as links. Other links become text.
public enum ItemSummary {
    /// `summary` with every bare `http(s)` URL, and every word or code span that names an
    /// attachment, made a link, outside code blocks. A written link to an attachment gets
    /// the attachment's `SummaryLink.url`, and an image becomes a link.
    public static func markdown(_ summary: String, attachments: Attachments) -> String {
        var fenced = false
        // "\r\n" is one Character, so splitting on "\n" alone would miss it.
        let lines = summary.replacingOccurrences(of: "\r\n", with: "\n")
            .split(separator: "\n", omittingEmptySubsequences: false)
        return lines.map { line in
            let start = line.drop { $0 == " " }
            let fence = start.hasPrefix("```") || start.hasPrefix("~~~")
            if fence { fenced.toggle() }
            if fence || fenced || line.hasPrefix("    ") || line.hasPrefix("\t") { return String(line) }
            return linkify(line, attachments)
        }.joined(separator: "\n")
    }

    private static func linkify(_ line: Substring, _ a: Attachments) -> String {
        var out = ""
        var i = line.startIndex
        while i < line.endIndex {
            if let (written, end) = existingLink(line[i...], a) {
                out += written
                i = end
            } else if line[i] == "`" {
                i = codeSpan(line, from: i, a, into: &out)
            } else if line[i].isWhitespace {
                out.append(line[i])
                i = line.index(after: i)
            } else {
                i = word(line, from: i, a, into: &out)
            }
        }
        return out
    }

    /// The `[text](dest)`, `![alt](dest)` or `<...>` that `text` starts with, as the
    /// summary shows it, and where it ends.
    private static func existingLink(_ text: Substring, _ a: Attachments) -> (String, String.Index)? {
        switch text.first {
        case "[", "!":
            guard let m = text.prefixMatch(of: #/!?\[([^\]]*)\]\(([^)\s]*)(?:\s+"[^"]*")?\)/#) else { return nil }
            let (_, label, dest) = m.output
            let written = String(dest)
            let link = URL(string: written).flatMap { SummaryLink($0) }
                ?? attachment(named: written.removingPercentEncoding ?? written, in: a)
            let shown = label.isEmpty ? escape(written) : String(label)
            return (link.map { "[\(shown)](<\($0.url.absoluteString)>)" } ?? String(label), m.range.upperBound)
        case "<":
            return text.prefixMatch(of: #/<[^<>\s]+>/#).map { (String($0.output), $0.range.upperBound) }
        default:
            return nil
        }
    }

    /// A code span; one that names an attachment becomes its link.
    private static func codeSpan(_ line: Substring, from start: String.Index, _ a: Attachments,
                                 into out: inout String) -> String.Index {
        let ticks = line[start...].prefix { $0 == "`" }
        guard let close = line[ticks.endIndex...].range(of: ticks) else {
            out += ticks
            return ticks.endIndex
        }
        let span = line[start..<close.upperBound]
        let name = line[ticks.endIndex..<close.lowerBound].trimmingCharacters(in: .whitespaces)
        out += attachment(named: name, in: a).map { "[\(span)](<\($0.url.absoluteString)>)" } ?? String(span)
        return close.upperBound
    }

    /// A run of non-space characters. Without the punctuation around it, a web URL or
    /// a name of an attachment becomes a link.
    private static func word(_ line: Substring, from start: String.Index, _ a: Attachments,
                             into out: inout String) -> String.Index {
        var end = line.index(after: start)
        while end < line.endIndex, !line[end].isWhitespace, line[end] != "`", existingLink(line[end...], a) == nil {
            end = line.index(after: end)
        }
        let raw = line[start..<end]
        let lead = raw.prefix { "(\"'*_".contains($0) }
        let rest = raw[lead.endIndex...]
        var core = rest
        while let last = core.last, ".,;:!?)\"'*_".contains(last) { core = core.dropLast() }
        let trail = rest[core.endIndex...]
        let lower = core.lowercased()
        let link: String? = if lower.hasPrefix("http://") || lower.hasPrefix("https://"), URL(string: String(core)) != nil {
            "<\(core)>"
        } else if !core.contains("://"), let found = attachment(named: String(core), in: a) {
            "[\(escape(String(core)))](<\(found.url.absoluteString)>)"
        } else {
            nil
        }
        out += link.map { lead + $0 + trail } ?? String(raw)
        return end
    }

    /// Markdown characters in a file name stay literal in the link's text.
    private static func escape(_ name: String) -> String {
        name.reduce(into: "") { out, c in
            if "\\_*[]".contains(c) { out.append("\\") }
            out.append(c)
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
