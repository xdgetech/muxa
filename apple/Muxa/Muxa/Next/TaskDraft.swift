import Foundation
import MuxaV2

/// What opens the Start Task sheet: empty, or prefilled from an Inbox item or a capture.
struct TaskSeed: Identifiable, Equatable {
    let id = UUID()
    var title = ""
    var goal: String?
    var workspace: String?
}

/// One `start-task` request (protocol v2, Task actions).
struct TaskRequest: Equatable {
    var workspace: String
    var title: String
    var goal: String?
    var places: [PlaceSpec]
    var plan: Bool
    var loop: LoopSpec?
}

/// The Start Task form's choices for one Workspace: one repo or none (ADR 0077).
struct TaskDraft {
    /// How the Task uses its one repo.
    enum RepoMode: String, CaseIterable, Identifiable {
        case branch = "Branch", read = "Read only"
        var id: Self { self }
    }

    var title: String
    var goal: String
    var workspace = ""
    /// Nil: no repo; the Task's folder, with every repo read only.
    var repo: String?
    var mode = RepoMode.branch
    var branch = ""
    var plan = false
    var loop = false
    var check = ""
    var attempts = 3

    init(seed: TaskSeed) {
        title = seed.title
        goal = seed.goal ?? ""
    }

    /// A new Workspace: its repos and checks differ.
    mutating func choose(_ ws: Workspace) {
        workspace = ws.name
        repo = nil
        mode = .branch
        branch = ""
        loop = false
        if case .list(let checks)? = ws.loop?.checks { check = checks.first ?? "" } else { check = "" }
    }

    /// What still keeps Start off, or nil.
    func missing(in ws: Workspace) -> String? {
        if trimmed(title).isEmpty { return "Name the Task." }
        if let repo, mode == .branch, trimmed(branch).isEmpty { return "Name the branch for \(repo)." }
        if loop, ws.loop?.checks == .any, trimmed(check).isEmpty { return "Type the loop's check." }
        return nil
    }

    func request(in ws: Workspace) -> TaskRequest {
        let places: [PlaceSpec]
        switch (repo, mode) {
        case (nil, _): places = [.allRead]
        case let (repo?, .branch): places = [.branch(repo: repo, branch: trimmed(branch))]
        case let (repo?, .read): places = [.read(repo: repo)]
        }
        let goal = trimmed(goal)
        return TaskRequest(
            workspace: ws.name, title: trimmed(title), goal: goal.isEmpty ? nil : goal, places: places,
            plan: plan,
            loop: loop && ws.loop != nil
                ? LoopSpec(check: trimmed(check).isEmpty ? nil : trimmed(check), maxAttempts: UInt32(attempts)) : nil)
    }

    private func trimmed(_ s: String) -> String { s.trimmingCharacters(in: .whitespacesAndNewlines) }
}
