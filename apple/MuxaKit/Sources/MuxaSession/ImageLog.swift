import Foundation

/// The open image, as `image-content` updates it.
public struct ImageLog: Equatable, Sendable {
    public let pane: String
    public let path: String
    /// The file as it is on the host; nil until it arrives.
    public internal(set) var data: Data?
    public internal(set) var mtime: UInt64?
    /// Why the bridge stopped following it, if it did.
    public internal(set) var ended: String?

    public init(pane: String, path: String) {
        self.pane = pane
        self.path = path
    }
}
