import MuxaProtocol

/// The open document, as `doc-content` updates it.
public struct DocLog: Equatable, Sendable {
    public let pane: String
    public let path: String
    /// Nil until the first content arrives.
    public internal(set) var text: String?
    /// Epoch seconds of the file's last change.
    public internal(set) var mtime: UInt64?
    /// Why the bridge stopped following it, if it did.
    public internal(set) var ended: String?

    public init(pane: String, path: String) {
        self.pane = pane
        self.path = path
    }
}
