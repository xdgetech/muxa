import Foundation
import MuxaProtocol

/// The demo bridge's task records (ADR 0015): one in each Inbox group, kept in memory.
/// The Android demo serves the same records.
struct DemoTasks: Sendable {
    var workspaces: [WorkspaceInfo]
    var records: [TaskRecord]
    /// Records created so far, for the next id.
    var created = 0

    init(workspaces: [WorkspaceInfo], records: [TaskRecord]) {
        self.workspaces = workspaces
        self.records = records
    }

    /// Where TASK.md lives in a task's directory, as the Reader lists it.
    static let taskFile = ".muxa/TASK.md"

    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    private static func record(
        _ id: String, _ workspace: String, _ title: String, _ goal: String, status: TaskStatus = .open,
        ago: UInt64, project: String? = nil, repo: String? = nil, branch: String? = nil, dir: String? = nil,
        session: String? = nil, note: String? = nil
    ) -> TaskRecord {
        TaskRecord(id: id, rev: 1, workspace: workspace, title: title, goal: goal, status: status,
                   createdAt: now - ago - 3600, updatedAt: now - ago, project: project, repo: repo,
                   branch: branch, dir: dir ?? repo.map { _ in "/src/.worktrees/\(id)" }, session: session, note: note)
    }

    private static let work = [WorkspaceInfo(name: "work"), WorkspaceInfo(name: "personal")]

    /// The UI tests' records, linked to `DemoBridge.initialSessions` by `taskId`.
    static let tests = DemoTasks(workspaces: work, records: [
        record("t-auth01", "work", "Add sign-in with passkeys", "Let users sign in with a passkey instead of a password.",
               ago: 90, project: "App", repo: "/src/app", branch: "feat/auth", dir: "/src/.worktrees/app-feat-auth",
               session: "feature-auth"),
        record("t-login1", "work", "Fix the login redirect", "After sign-in, return users to the page they came from.",
               ago: 600, project: "App", repo: "/src/app", branch: "fix/login", dir: "/src/.worktrees/app-fix-login",
               session: "fix-login"),
        record("t-docs01", "work", "Update the docs site", "Document the new settings page.",
               ago: 20, project: "Docs", repo: "/src/docs", branch: "main", dir: "/src/docs", session: "docs-site"),
        record("t-price1", "work", "Draft the pricing page", "Write the copy for the new pricing page, with three plans.",
               ago: 1200, project: "Launch", dir: "/work/tasks/draft-the-pricing-page-t-price1",
               note: "The agent did not start: no agent command is set for this workspace."),
        record("t-relnot", "work", "Write the release notes", "Summarize the changes since 1.2 for the blog.",
               status: .done, ago: 86_400, project: "Launch", dir: "/work/tasks/write-the-release-notes-t-relnot"),
    ])

    /// Try the Demo and the store screenshots, linked to `Showcase.sessions`.
    static let showcase = DemoTasks(workspaces: work, records: [
        record("t-a1b2c3", "work", "Handle Stripe invoice.paid webhook",
               "Mark the invoice paid when Stripe sends invoice.paid, and email a receipt.",
               ago: 75, project: "Billing", repo: "~/src/billing", branch: "feat/stripe-webhooks",
               dir: "~/src/.worktrees/billing-stripe-webhooks", session: "stripe-webhooks"),
        record("t-d4e5f6", "work", "Retry failed charges with backoff",
               "Retry a failed charge three times, with exponential backoff and jitter.",
               ago: 40, project: "Billing", repo: "~/src/billing", branch: "fix/retry-backoff",
               dir: "~/src/.worktrees/billing-retry-backoff", session: "retry-backoff"),
        record("t-0a1b2c", "work", "Dark mode for the dashboard", "Add a dark theme that follows the system setting.",
               ago: 720, project: "Dashboard", repo: "~/src/dashboard", branch: "feat/dark-mode",
               dir: "~/src/.worktrees/dashboard-dark-mode", session: "dark-mode"),
        record("t-3c4d5e", "work", "Faster search index", "Cut the search index build time in half.",
               status: .review, ago: 1800, project: "Dashboard", repo: "~/src/dashboard", branch: "perf/search-index",
               dir: "~/src/.worktrees/dashboard-search-index", session: "search-index"),
        record("t-6f7a8b", "work", "Plan the Q3 pricing change",
               "Compare our plans with three competitors and propose new prices.",
               ago: 5400, project: "Pricing", dir: "~/work/tasks/plan-the-q3-pricing-change-t-6f7a8b"),
        record("t-9b8c7d", "personal", "File the VAT return", "Collect the quarter's invoices and file the VAT return.",
               status: .done, ago: 172_800, dir: "~/personal/tasks/file-the-vat-return-t-9b8c7d"),
    ])

    func record(_ id: String?) -> TaskRecord? {
        records.first { $0.id == id }
    }

    /// TASK.md as the bridge writes it: the goal, the workspace and the resources.
    func taskFile(for id: String?) -> String? {
        guard let r = record(id) else { return nil }
        var text = "# \(r.title)\n\n## Goal\n\n\(r.goal)\n\n## Workspace\n\n\(r.workspace)"
        if let project = r.project { text += " · \(project)" }
        if let repo = r.repo { text += "\n\n## Resources\n\n- Repo: `\(repo)`, branch `\(r.branch ?? "")`" }
        return text + "\n"
    }
}

// MARK: Writes, with the bridge's checks

extension DemoTasks {
    enum Refusal: Error {
        case badRequest(String)
        case conflict
    }

    /// A new record, or why the bridge refuses it. `launched` is false when the launch fails.
    mutating func create(
        workspace: String, title: String, goal: String, project: String?, repo: String?, branch: String?, launched: Bool
    ) throws(Refusal) -> TaskRecord {
        guard workspaces.contains(where: { $0.name == workspace }) else { throw .badRequest("\(workspace) is not a workspace") }
        if repo != nil, branch == nil { throw .badRequest("a task with a repo needs a branch") }
        created += 1
        let id = String(format: "t-new%03d", created)
        let slug = Self.slug(title)
        let dir = repo.map { _ in "/src/.worktrees/\(slug)" } ?? "/\(workspace)/tasks/\(slug)-\(id)"
        let record = TaskRecord(
            id: id, rev: 1, workspace: workspace, title: title, goal: goal, status: .open,
            createdAt: Self.now, updatedAt: Self.now, project: project, repo: repo, branch: branch, dir: dir,
            session: launched ? slug : nil,
            note: launched ? nil : "could not create a worktree for \(branch ?? ""): a branch named '\(branch ?? "")' already exists")
        records.append(record)
        return record
    }

    mutating func update(_ id: String, rev: UInt64, status: TaskStatus) throws(Refusal) -> TaskRecord {
        guard let i = records.firstIndex(where: { $0.id == id }) else { throw .badRequest("\(id) is not a task") }
        guard records[i].rev == rev else { throw .conflict }
        records[i].rev += 1
        records[i].status = status
        records[i].updatedAt = Self.now
        return records[i]
    }

    /// `[a-z0-9-]`, at most 20 characters, like the bridge's slugs but shorter for a session name.
    static func slug(_ title: String) -> String {
        let words = title.lowercased().split { !($0.isASCII && ($0.isLetter || $0.isNumber)) }
        let slug = String(words.joined(separator: "-").prefix(20))
        return slug.hasSuffix("-") ? String(slug.dropLast()) : (slug.isEmpty ? "task" : slug)
    }
}
