import Foundation

/// An image file in the pane's repo, ignored ones included.
public struct ImageInfo: Codable, Equatable, Identifiable, Sendable {
    /// Relative to the repo; what to send in `image`.
    public var path: String
    /// Epoch seconds of the last change.
    public var mtime: UInt64?
    /// Bytes.
    public var size: UInt64?

    public var id: String { path }

    public init(path: String, mtime: UInt64? = nil, size: UInt64? = nil) {
        self.path = path
        self.mtime = mtime
        self.size = size
    }
}

/// The body of an `image-list` message.
public struct ImageListing: Codable, Equatable, Sendable {
    /// The id of the `images` request.
    public var images: UInt64
    /// The repo's top directory name.
    public var root: String?
    /// Newest first.
    public var entries: [ImageInfo]
    /// More images exist than were listed.
    public var truncated: Bool

    public init(images: UInt64, root: String? = nil, entries: [ImageInfo], truncated: Bool = false) {
        self.images = images
        self.root = root
        self.entries = entries
        self.truncated = truncated
    }

    private enum Key: String, CodingKey { case images, root, entries, truncated }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        images = try c.decode(UInt64.self, forKey: .images)
        root = try c.decodeIfPresent(String.self, forKey: .root)
        entries = try c.decode([ImageInfo].self, forKey: .entries)
        truncated = try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(images, forKey: .images)
        try c.encodeIfPresent(root, forKey: .root)
        try c.encode(entries, forKey: .entries)
        if truncated { try c.encode(true, forKey: .truncated) }
    }
}
