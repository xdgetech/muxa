import Foundation

/// A Markdown document a pane may read: one of its repo's, or one its agent writes.
public struct DocInfo: Codable, Equatable, Identifiable, Sendable {
    public enum Kind: String, Codable, Sendable {
        case repo
        /// Written by the pane's agent outside the repo, e.g. a plan.
        case plan
    }

    /// What to send in `doc`: relative to the repo, or `plans/<name>`.
    public var path: String
    public var kind: Kind
    public var title: String?
    /// Epoch seconds of the last change.
    public var mtime: UInt64?

    public var id: String { path }

    public init(path: String, kind: Kind, title: String? = nil, mtime: UInt64? = nil) {
        self.path = path
        self.kind = kind
        self.title = title
        self.mtime = mtime
    }
}

/// The body of a `doc-list` message.
public struct DocListing: Codable, Equatable, Sendable {
    /// The id of the `docs` request.
    public var docs: UInt64
    /// The repo's top directory name.
    public var root: String?
    public var entries: [DocInfo]
    /// More documents exist than were listed.
    public var truncated: Bool

    public init(docs: UInt64, root: String? = nil, entries: [DocInfo], truncated: Bool = false) {
        self.docs = docs
        self.root = root
        self.entries = entries
        self.truncated = truncated
    }

    private enum Key: String, CodingKey { case docs, root, entries, truncated }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        docs = try c.decode(UInt64.self, forKey: .docs)
        root = try c.decodeIfPresent(String.self, forKey: .root)
        entries = try c.decode([DocInfo].self, forKey: .entries)
        truncated = try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(docs, forKey: .docs)
        try c.encodeIfPresent(root, forKey: .root)
        try c.encode(entries, forKey: .entries)
        if truncated { try c.encode(true, forKey: .truncated) }
    }
}
