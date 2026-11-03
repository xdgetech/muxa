import Foundation
import MuxaProtocol

/// A repo the New Task sheet offers: a workspace's main checkout (ADR 0031).
public struct RepoChoice: Hashable, Sendable, Identifiable {
    /// The main checkout, absolute. This is what the sheet sends.
    public var dir: String
    /// Relative to the workspace root when under it, else `~`-abbreviated.
    public var name: String
    public var id: String { dir }

    public init(dir: String, name: String) {
        self.dir = dir
        self.name = name
    }

    /// `dir` relative to `root` when under it, else with the home folder as `~`.
    /// The client does not know the host's home, so it reads it from the path.
    public static func displayName(_ dir: String, root: String?) -> String {
        if let root, !root.isEmpty {
            let prefix = root.hasSuffix("/") ? root : root + "/"
            if dir.hasPrefix(prefix), dir.count > prefix.count { return String(dir.dropFirst(prefix.count)) }
        }
        return abbreviatingHome(dir)
    }

    private static func abbreviatingHome(_ path: String) -> String {
        let parts = path.split(separator: "/", omittingEmptySubsequences: false)
        // "", "Users", "<name>", ...
        if parts.count >= 3, parts[0].isEmpty, ["Users", "home"].contains(parts[1]), !parts[2].isEmpty {
            return (["~"] + parts.dropFirst(3)).joined(separator: "/")
        }
        if parts.count >= 2, parts[0].isEmpty, parts[1] == "root" {
            return (["~"] + parts.dropFirst(2)).joined(separator: "/")
        }
        return path
    }
}

extension Array where Element == WorkspaceInfo {
    /// Whether the host lists repos per workspace; older hosts do not.
    public var listRepos: Bool { contains { !$0.repos.isEmpty } }

    /// The repos to offer in `workspace`. A host that lists none falls back to the
    /// repos the host has sessions in.
    public func repoChoices(in workspace: String, known: [KnownRepo]) -> [RepoChoice] {
        guard listRepos else { return known.map { RepoChoice(dir: $0.dir, name: $0.name) } }
        guard let info = first(where: { $0.name == workspace }) else { return [] }
        return info.repos.map { RepoChoice(dir: $0, name: RepoChoice.displayName($0, root: info.root)) }
    }

    /// The workspace a repo belongs to: one that lists it, else one whose root holds it.
    public func workspace(of dir: String) -> WorkspaceInfo? {
        first { $0.repos.contains(dir) } ?? first { info in
            guard let root = info.root, !root.isEmpty else { return false }
            return dir.hasPrefix(root.hasSuffix("/") ? root : root + "/")
        }
    }
}
