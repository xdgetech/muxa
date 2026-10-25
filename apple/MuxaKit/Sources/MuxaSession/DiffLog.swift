import MuxaProtocol

/// The open diff, as `diff-content` updates it.
public struct DiffLog: Equatable, Sendable {
    public let pane: String
    public let path: String
    /// The unified diff; nil until it arrives, and for a binary file.
    public internal(set) var text: String?
    public internal(set) var binary = false
    /// Why the bridge stopped following it, if it did.
    public internal(set) var ended: String?

    public init(pane: String, path: String) {
        self.pane = pane
        self.path = path
    }

    /// Something arrived: text, or word that the file is binary.
    public var loaded: Bool { text != nil || binary }
}
