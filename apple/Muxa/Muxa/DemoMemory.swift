import CryptoKit
import Foundation
import MuxaProtocol

/// The demo's workspace memory (`work` level 14, ADR 0053), with the host's checks: approve
/// needs the proposal's hash and appends once.
extension DemoWork {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// The host's refusal of an approve without the proposal's hash (PROTOCOL.md, "Workspace memory").
    static let memoryNotShown = "this app cannot show the whole memory text; update the app or use muxa-cli"

    /// Approve on a `memory` item. `appended` is false when an earlier approve appended it.
    mutating func approveMemory(_ id: String, hash: String?) throws(Refusal) -> (item: InboxItem, appended: Bool) {
        guard let i = taskItems.firstIndex(where: { $0.id == id && $0.kind == .memory }), let memory = taskItems[i].memory else {
            throw Refusal(.badRequest, "\(id) is not a memory item")
        }
        guard let hash, hash == memory.hash else { throw Refusal(.badRequest, Self.memoryNotShown) }
        if taskItems[i].state == .answered, taskItems[i].answer == .approve { return (taskItems[i], false) }
        guard taskItems[i].state == .open, taskItems[i].answers.contains(.approve) else {
            throw Refusal(.badRequest, "\(id) is \(taskItems[i].state)")
        }
        taskItems[i].state = .answered
        taskItems[i].answer = .approve
        taskItems[i].answeredAt = Self.now
        taskItems[i].answers = []
        return (taskItems[i], true)
    }

    /// As a level 13 host keeps it: no `memory` items.
    mutating func forgetMemory() {
        taskItems.removeAll { $0.kind == .memory }
    }

    /// Adds a `memory` item for `task`'s end; `approved` makes it answered.
    func proposingMemory(
        _ id: String, task: String, title: String, workspace: String, text: String, ago: UInt64, approved: Bool = false
    ) -> DemoWork {
        var w = self
        let proposal = MemoryProposal(task: task, title: title, text: text,
                                      hash: Self.memoryHash(task: task, title: title, text: text))
        w.taskItems.append(InboxItem(
            id: id, kind: .memory, state: approved ? .answered : .open,
            question: "\(title) proposes to add to the workspace memory", workspace: workspace, title: title,
            createdAt: Self.now - ago, answeredAt: approved ? Self.now - ago + 60 : nil, answer: approved ? .approve : nil,
            task: task, memory: proposal, answers: approved ? [] : [.approve, .reject, .later]))
        return w
    }

    /// `sha256:` and the hex SHA-256 of `task`, NUL, `title`, NUL, `text`, as the host hashes it.
    static func memoryHash(task: String, title: String, text: String) -> String {
        let digest = SHA256.hash(data: Data((task + "\0" + title + "\0" + text).utf8))
        return "sha256:" + digest.map { String(format: "%02x", $0) }.joined()
    }
}

extension DemoTasks {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// As a level 13 host lists them: no workspace has a memory mode.
    mutating func forgetMemory() {
        for i in workspaces.indices {
            workspaces[i].memory = nil
            workspaces[i].memoryAt = nil
        }
    }

    /// An approve appended to the workspace's WORKSPACE.md.
    mutating func memoryChanged(_ workspace: String) {
        guard let i = workspaces.firstIndex(where: { $0.name == workspace }) else { return }
        workspaces[i].memoryAt = Self.now
    }

    /// `work` reviews each addition and its file changed three hours ago, `personal` adds
    /// at once, `notes` keeps none; and `t-keys01`, a done Task whose end proposes memory.
    func remembering() -> DemoTasks {
        var t = self
        let modes: [String: (MemoryMode, UInt64?)] = [
            "work": (.review, Self.now - 3 * 3600), "personal": (.auto, nil), "notes": (.off, nil),
        ]
        for i in t.workspaces.indices {
            guard let (mode, at) = modes[t.workspaces[i].name] else { continue }
            t.workspaces[i].memory = mode
            t.workspaces[i].memoryAt = at
        }
        t.records.append(TaskRecord(
            id: "t-keys01", rev: 1, workspace: "work", title: DemoMemory.keysTitle,
            goal: "Rotate the API keys and write down how.", status: .done, createdAt: Self.now - 9000,
            updatedAt: Self.now - 1800, project: "App", dir: "/work/tasks/rotate-the-api-keys-t-keys01"))
        return t
    }
}

// MARK: Fixtures

enum DemoMemory {
    static let keysTitle = "Rotate the API keys"

    /// The UI tests' 4 KiB text: 102 numbered lines of 40 bytes, 4080 bytes in all.
    static let longText = (1...102).map { String(format: "Note %03d: rotate keys with make rotate.\n", $0) }.joined()

    static let releaseText = "- The release notes live in docs/releases/. One file per minor version.\n"

    static let vatText = "- The VAT return is due on the 10th of the month after the quarter.\n"
        + "- Invoices are in ~/personal/invoices, one folder per quarter.\n"
}

extension DemoWork {
    /// The UI tests' `-demo-memory`: t-keys01's end proposes i-0000e001, the 4 KiB text;
    /// t-relnot's proposal i-0000e002 was approved.
    static var testsMemory: DemoWork {
        tests
            .proposingMemory("i-0000e001", task: "t-keys01", title: DemoMemory.keysTitle, workspace: "work",
                             text: DemoMemory.longText, ago: 1700)
            .proposingMemory("i-0000e002", task: "t-relnot", title: "Write the release notes", workspace: "work",
                             text: DemoMemory.releaseText, ago: 10_700, approved: true)
    }

    static var showcaseMemory: DemoWork {
        showcaseWriting.proposingMemory("i-5c00e001", task: "t-9b8c7d", title: "File the VAT return",
                                        workspace: "personal", text: DemoMemory.vatText, ago: 21_000)
    }
}

extension DemoContent {
    static var testsMemory: DemoContent {
        var content = tests
        content.tasks = content.tasks.remembering()
        content.work = .testsMemory
        return content
    }
}
