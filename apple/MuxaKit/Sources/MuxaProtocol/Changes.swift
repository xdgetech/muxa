import Foundation

/// A file of the pane's repo that differs from HEAD.
public struct ChangeInfo: Codable, Equatable, Identifiable, Sendable {
    public enum Status: String, Codable, Sendable {
        case modified, added, deleted, renamed, untracked, conflicted
        /// A status added by a newer bridge.
        case other

        public init(from decoder: any Decoder) throws {
            let raw = try decoder.singleValueContainer().decode(String.self)
            self = Status(rawValue: raw) ?? .other
        }
    }

    /// Relative to the repo; what to send in `diff`.
    public var path: String
    public var status: Status
    /// The path before a rename.
    public var oldPath: String?
    /// Lines added and removed; nil for a binary file.
    public var added: UInt32?
    public var removed: UInt32?

    public var id: String { path }

    public init(path: String, status: Status, oldPath: String? = nil, added: UInt32? = nil, removed: UInt32? = nil) {
        self.path = path
        self.status = status
        self.oldPath = oldPath
        self.added = added
        self.removed = removed
    }
}

/// Which list a `diff` path is from.
public enum DiffScope: String, Codable, Equatable, Hashable, Sendable {
    /// Against HEAD: staged, unstaged and untracked.
    case uncommitted
    /// The branch's commits since it left its base.
    case branch
}

/// The files a branch's commits changed since it left its base.
public struct BranchChanges: Codable, Equatable, Sendable {
    /// The ref compared with, e.g. `origin/main`.
    public var base: String
    /// Commits since the fork point.
    public var commits: UInt32
    public var entries: [ChangeInfo]
    public var truncated: Bool

    public init(base: String, commits: UInt32, entries: [ChangeInfo], truncated: Bool = false) {
        self.base = base
        self.commits = commits
        self.entries = entries
        self.truncated = truncated
    }

    private enum Key: String, CodingKey { case base, commits, entries, truncated }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        base = try c.decode(String.self, forKey: .base)
        commits = try c.decode(UInt32.self, forKey: .commits)
        entries = try c.decode([ChangeInfo].self, forKey: .entries)
        truncated = try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(base, forKey: .base)
        try c.encode(commits, forKey: .commits)
        try c.encode(entries, forKey: .entries)
        if truncated { try c.encode(true, forKey: .truncated) }
    }
}

/// The body of a `change-list` message.
public struct ChangeListing: Codable, Equatable, Sendable {
    /// The id of the `changes` request.
    public var changes: UInt64
    /// The repo's top directory name.
    public var root: String?
    /// Against HEAD: staged, unstaged and untracked.
    public var entries: [ChangeInfo]
    /// More files changed than were listed.
    public var truncated: Bool
    /// What the branch's commits changed; nil on the base itself or without one.
    public var branch: BranchChanges?

    public init(changes: UInt64, root: String? = nil, entries: [ChangeInfo], truncated: Bool = false,
                branch: BranchChanges? = nil) {
        self.changes = changes
        self.root = root
        self.entries = entries
        self.truncated = truncated
        self.branch = branch
    }

    private enum Key: String, CodingKey { case changes, root, entries, truncated, branch }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        changes = try c.decode(UInt64.self, forKey: .changes)
        root = try c.decodeIfPresent(String.self, forKey: .root)
        entries = try c.decode([ChangeInfo].self, forKey: .entries)
        truncated = try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false
        branch = try c.decodeIfPresent(BranchChanges.self, forKey: .branch)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        try c.encode(changes, forKey: .changes)
        try c.encodeIfPresent(root, forKey: .root)
        try c.encode(entries, forKey: .entries)
        if truncated { try c.encode(true, forKey: .truncated) }
        try c.encodeIfPresent(branch, forKey: .branch)
    }
}
