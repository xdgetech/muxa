import Foundation

// External writes only when asked (ADR 0052), at `work` level 13. A Result proposes writes; the
// user approves each exact text in the Inbox; one writer worker performs it as a Task.

/// A write a Result proposes, as the host stored it.
public struct ResultWrite: Equatable, Sendable {
    /// 1 to 200 characters on one line, e.g. "Post to #release".
    public var action: String
    /// 1 to 500 bytes on one line: `slack:#release`, `linear:ENG-12`, a URL.
    public var target: String
    /// The exact text to write, 1 byte to 8 KiB.
    public var content: String
    /// `sha256:` and the hex SHA-256 of `action`, NUL, `target`, NUL, `content`.
    public var hash: String
    /// Why the member proposes it.
    public var reason: String?
    /// The `write` Inbox item that asks the user, once made.
    public var item: String?
    /// The writer's Task, once approved.
    public var task: String?

    public init(
        action: String, target: String, content: String, hash: String, reason: String? = nil, item: String? = nil,
        task: String? = nil
    ) {
        self.action = action
        self.target = target
        self.content = content
        self.hash = hash
        self.reason = reason
        self.item = item
        self.task = task
    }
}

extension ResultWrite: Codable {
    private enum Key: String, CodingKey { case action, target, content, hash, reason, item, task }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        action = try c.decode(String.self, forKey: .action)
        target = try c.decode(String.self, forKey: .target)
        content = try c.decode(String.self, forKey: .content)
        hash = try c.decode(String.self, forKey: .hash)
        reason = try c.decodeIfPresent(String.self, forKey: .reason)
        item = try c.decodeIfPresent(String.self, forKey: .item)
        task = try c.decodeIfPresent(String.self, forKey: .task)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(action, forKey: .action)
        try c.encode(target, forKey: .target)
        try c.encode(content, forKey: .content)
        try c.encode(hash, forKey: .hash)
        try c.encodeIfPresent(reason, forKey: .reason)
        try c.encodeIfPresent(item, forKey: .item)
        try c.encodeIfPresent(task, forKey: .task)
    }
}

/// A `write` Inbox item's payload: the exact write approve hands the writer.
public struct WriteProposal: Equatable, Sendable {
    /// `q-…`.
    public var request: String
    /// The request version whose Result proposed it; may be older than the current one.
    public var version: UInt64
    /// The step of `version` whose Result proposed it. Nil: step 1.
    public var step: UInt32?
    /// The staff member whose Result proposed it. Nil: the workspace's agent.
    public var worker: String?
    public var action: String
    public var target: String
    /// Shown whole, as plain text, before Approve.
    public var content: String
    public var hash: String

    public init(
        request: String, version: UInt64, step: UInt32? = nil, worker: String? = nil, action: String, target: String,
        content: String, hash: String
    ) {
        self.request = request
        self.version = version
        self.step = step
        self.worker = worker
        self.action = action
        self.target = target
        self.content = content
        self.hash = hash
    }
}

extension WriteProposal: Codable {
    private enum Key: String, CodingKey { case request, version, step, worker, action, target, content, hash }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        request = try c.decode(String.self, forKey: .request)
        version = try c.decode(UInt64.self, forKey: .version)
        step = try c.decodeIfPresent(UInt32.self, forKey: .step)
        worker = try c.decodeIfPresent(String.self, forKey: .worker)
        action = try c.decode(String.self, forKey: .action)
        target = try c.decode(String.self, forKey: .target)
        content = try c.decode(String.self, forKey: .content)
        hash = try c.decode(String.self, forKey: .hash)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(request, forKey: .request)
        try c.encode(version, forKey: .version)
        try c.encodeIfPresent(step, forKey: .step)
        try c.encodeIfPresent(worker, forKey: .worker)
        try c.encode(action, forKey: .action)
        try c.encode(target, forKey: .target)
        try c.encode(content, forKey: .content)
        try c.encode(hash, forKey: .hash)
    }
}

/// The worker that performs a workspace's approved writes, in `inbox-list`.
public struct WriterInfo: Codable, Equatable, Sendable {
    /// A host `[[worker]]` name.
    public var worker: String
    /// The worker's job description, at most 1 KiB.
    public var description: String

    public init(worker: String, description: String) {
        self.worker = worker
        self.description = description
    }
}

/// Whether a workspace takes write proposals. A value added by a newer bridge decodes as
/// `other`: show it as unknown. Compare with `Writes.none`, not `.none`, on an optional.
public enum Writes: Equatable, Hashable, Sendable {
    /// A Result with `writes` fails the check; changes go into `next`.
    case none
    /// The user approves each write; the writer performs it.
    case review
    case other(String)

    private static let names: [(Writes, String)] = [(.none, "none"), (.review, "review")]

    public init(raw: String) {
        self = Self.names.first { $0.1 == raw }?.0 ?? .other(raw)
    }

    public var raw: String {
        if case .other(let raw) = self { return raw }
        return Self.names.first { $0.0 == self }?.1 ?? ""
    }
}

extension Writes: Codable {
    public init(from decoder: any Decoder) throws {
        self.init(raw: try decoder.singleValueContainer().decode(String.self))
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(raw)
    }
}
