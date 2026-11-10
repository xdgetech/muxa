import Foundation

// Workspace memory (ADR 0053), at `work` level 14. A Task's end proposes the text under "For the
// workspace" in its TASK.md; the user approves it in the Inbox before the host appends it to
// `<root>/.muxa/WORKSPACE.md`.

/// A `memory` Inbox item's payload: the exact text approve appends.
public struct MemoryProposal: Equatable, Sendable {
    /// The Task whose close-out proposed it.
    public var task: String
    /// The Task's title; the appended heading carries it.
    public var title: String
    /// Shown whole, as plain text, before Approve. At most 4 KiB.
    public var text: String
    /// `sha256:` and the hex SHA-256 of `task`, NUL, `title`, NUL, `text`.
    public var hash: String

    public init(task: String, title: String, text: String, hash: String) {
        self.task = task
        self.title = title
        self.text = text
        self.hash = hash
    }
}

extension MemoryProposal: Codable {
    private enum Key: String, CodingKey { case task, title, text, hash }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        task = try c.decode(String.self, forKey: .task)
        title = try c.decode(String.self, forKey: .title)
        text = try c.decode(String.self, forKey: .text)
        hash = try c.decode(String.self, forKey: .hash)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(task, forKey: .task)
        try c.encode(title, forKey: .title)
        try c.encode(text, forKey: .text)
        try c.encode(hash, forKey: .hash)
    }
}

/// What a workspace does with a Task's "For the workspace" at its end. A value added by a newer
/// bridge decodes as `other`: show it as unknown.
public enum MemoryMode: Equatable, Hashable, Sendable {
    /// The user approves each addition in the Inbox.
    case review
    /// The host appends at once, within the cap; otherwise it asks as `review`.
    case auto
    /// Nothing is read and nothing is made.
    case off
    case other(String)

    private static let names: [(MemoryMode, String)] = [(.review, "review"), (.auto, "auto"), (.off, "off")]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

extension MemoryMode: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}
