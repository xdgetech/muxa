import MuxaProtocol

/// A repo the host already has sessions for.
public struct KnownRepo: Hashable, Sendable, Identifiable {
    public var name: String
    /// The main checkout, absolute.
    public var dir: String
    public var id: String { dir }

    public init(name: String, dir: String) {
        self.name = name
        self.dir = dir
    }
}

extension Array where Element == Session {
    /// Each repo once, in the order its first session appears.
    public var knownRepos: [KnownRepo] {
        var seen: Set<String> = []
        return compactMap { s in
            guard let dir = s.repoDir, !dir.isEmpty, seen.insert(dir).inserted else { return nil }
            let name = s.repo ?? dir.split(separator: "/").last.map(String.init) ?? dir
            return KnownRepo(name: name, dir: dir)
        }
    }
}

/// What the New Task sheet edits. Checked here for quick feedback; the bridge
/// checks again and has the final say.
public struct TaskDraft: Equatable, Sendable {
    public var repoDir: String
    public var branch: String
    /// Empty: the repo's default branch.
    public var base: String

    public init(repoDir: String = "", branch: String = "", base: String = "") {
        self.repoDir = repoDir
        self.branch = branch
        self.base = base
    }

    /// What to fix before the task can start; nil when it can.
    public var problem: String? {
        let repo = repoDir.trimmingCharacters(in: .whitespaces)
        if repo.isEmpty { return "Pick a repository." }
        if !repo.hasPrefix("/") { return "The repository path must start with /." }
        let branch = branch.trimmingCharacters(in: .whitespaces)
        if branch.isEmpty { return "Name the branch." }
        if let why = Self.refProblem(branch, what: "branch") { return why }
        let base = base.trimmingCharacters(in: .whitespaces)
        if !base.isEmpty, let why = Self.refProblem(base, what: "base") { return why }
        return nil
    }

    /// The `new-task` fields, trimmed; nil while there is a problem.
    public var request: (repo: String, branch: String, base: String?)? {
        guard problem == nil else { return nil }
        let base = base.trimmingCharacters(in: .whitespaces)
        return (
            repoDir.trimmingCharacters(in: .whitespaces),
            branch.trimmingCharacters(in: .whitespaces),
            base.isEmpty ? nil : base
        )
    }

    /// The common mistakes git would refuse; `git check-ref-format` on the host decides the rest.
    private static func refProblem(_ name: String, what: String) -> String? {
        if name.hasPrefix("-") { return "A \(what) name cannot start with -." }
        if name.contains(where: { $0 == " " || $0 == "~" || $0 == "^" || $0 == ":" || $0 == "\\" }) || name.contains("..") {
            return "A \(what) name cannot contain spaces, .., ~, ^, : or \\."
        }
        return nil
    }
}
