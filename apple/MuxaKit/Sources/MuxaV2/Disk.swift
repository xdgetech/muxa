import Foundation

/// One row of `disk` (ADR 0102): a Task place, another worktree or a build
/// folder, with its size in bytes.
public struct DiskRow: Codable, Equatable, Sendable, Identifiable {
    public var path: String
    /// `task-worktree`, `task-folder`, `worktree`, `claude-worktree` or `build-folder`.
    public var kind: String
    public var workspace: String?
    public var branch: String?
    public var bytes: UInt64?
    public var buildBytes: UInt64?
    /// `live` or `ended`; nil when nothing holds it.
    public var held: String?
    public var clean: Bool?
    public var merged: Bool?
    /// `diskClean` without `force` removes it.
    public var cleanable: Bool

    public var id: String { path }
}

/// A row `diskClean` did not remove, and why.
public struct DiskKept: Codable, Equatable, Sendable {
    public var path: String
    public var why: String
}

/// What `diskClean` removed, or would remove on a dry run.
public struct DiskCleaned: Codable, Equatable, Sendable {
    public var dryRun: Bool
    public var removed: [String]
    public var kept: [DiskKept]
}
