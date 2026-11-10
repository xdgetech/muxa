import Foundation
import MuxaProtocol

/// The demo's write proposals (`work` level 13, ADR 0052), with the host's checks: approve
/// needs the write's hash, makes one Task of the workspace's writer, and a second approve
/// returns it.
extension DemoWork {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// The host's refusal of an approve without the write's hash (PROTOCOL.md, "Write proposals").
    static let writeNotShown = "this app cannot show the whole write; update the app or use muxa-cli"

    /// Approve on a `write` item: the writer's Task, made once. `made` is false when an
    /// earlier approve made it.
    mutating func approveWrite(
        _ id: String, hash: String?, tasks: inout DemoTasks
    ) throws(Refusal) -> (record: TaskRecord, item: InboxItem, made: Bool) {
        guard let i = taskItems.firstIndex(where: { $0.id == id && $0.kind == .write }), let write = taskItems[i].write else {
            throw Refusal(.badRequest, "\(id) is not a write item")
        }
        guard let hash, hash == write.hash else { throw Refusal(.badRequest, Self.writeNotShown) }
        if let task = taskItems[i].task, let record = tasks.record(task) { return (record, taskItems[i], false) }
        guard taskItems[i].state == .open else { throw Refusal(.badRequest, "\(id) is \(taskItems[i].state)") }
        let workspace = taskItems[i].workspace
        guard let writer = assistant.writers[workspace] else {
            throw Refusal(.conflict, "workspace \"\(workspace)\" has no writer")
        }
        let request = assistant.requests.first { $0.id == write.request }
        let record: TaskRecord
        do throws(DemoTasks.Refusal) {
            record = try tasks.create(workspace: workspace, title: write.action, goal: Self.writeGoal(id, write),
                                      project: nil, repo: nil, branch: nil, loop: nil, launched: true)
        } catch {
            throw Refusal(.conflict, "the writer \(writer.worker) cannot start")
        }
        let made = tasks.attach(record.id, worker: writer.worker, work: request?.work)
        if let work = request?.work, let w = assistant.work.firstIndex(where: { $0.id == work }) {
            assistant.work[w].tasks.append(made.id)
        }
        assistant.writeStarted(write.hash, request: write.request, task: made.id)
        taskItems[i].state = .answered
        taskItems[i].answer = .approve
        taskItems[i].answeredAt = Self.now
        taskItems[i].answers = []
        taskItems[i].task = made.id
        return (made, taskItems[i], true)
    }

    /// The writer's goal, as the host builds it: the content unchanged, the marker last.
    private static func writeGoal(_ item: String, _ write: WriteProposal) -> String {
        """
        Perform this approved action. Do exactly this and nothing else.

        \(write.action)
        Target: \(write.target)

        \(write.content)

        Approved in Inbox from a result of \(write.worker ?? "the workspace's agent").
        \(write.hash)
        muxa-write: \(item) \(write.hash)
        """
    }

    /// As a level 12 host keeps it: no writer, no writes on Results, no `write` items.
    mutating func forgetWrites() {
        assistant.writers = [:]
        assistant.listsWrites = false
        for r in assistant.requests.indices {
            for k in assistant.requests[r].results.indices { assistant.requests[r].results[k].writes = [] }
        }
        taskItems.removeAll { $0.kind == .write }
    }

    /// The writer `work` workspace has, and two writes: `first` on `request`'s newest
    /// result, and `second` on `older`'s result of `version` (an older version of its
    /// request), or on `request`'s too.
    func writing(
        request: String, items: (first: String, second: String), older: (request: String, version: UInt64)?,
        long: Bool
    ) -> DemoWork {
        var w = self
        w.assistant.writers["work"] = DemoWrites.writer
        let first = DemoWrites.post
        let second = long ? DemoWrites.longComment : DemoWrites.comment
        w.propose(first, on: request, version: nil, item: items.first)
        w.propose(second, on: older?.request ?? request, version: older?.version, item: items.second)
        return w
    }

    /// Adds `write` to the request's result of `version` (else its newest) and its `write` item.
    private mutating func propose(_ write: ResultWrite, on request: String, version: UInt64?, item id: String) {
        guard let r = assistant.requests.firstIndex(where: { $0.id == request }) else { return }
        let results = assistant.requests[r].results
        guard let k = version.flatMap({ v in results.firstIndex { $0.version == v } })
            ?? results.indices.max(by: { results[$0].at < results[$1].at })
        else { return }
        var stored = write
        stored.item = id
        assistant.requests[r].results[k].writes.append(stored)
        let result = assistant.requests[r].results[k]
        let worker = result.worker
        let at = result.at
        taskItems.insert(InboxItem(
            id: id, kind: .write, state: .open,
            question: "\(worker ?? "work's assistant") proposes to write to \(write.target)",
            workspace: assistant.requests[r].workspace, title: write.action, createdAt: at, summary: write.reason,
            write: WriteProposal(request: request, version: result.version, step: result.step, worker: worker,
                                 action: write.action, target: write.target, content: write.content, hash: write.hash),
            answers: [.approve, .reject, .later]), at: 0)
    }
}

extension DemoAssistant {
    /// Records the writer's Task on the Result's write.
    mutating func writeStarted(_ hash: String, request: String, task: String) {
        guard let r = requests.firstIndex(where: { $0.id == request }) else { return }
        for k in requests[r].results.indices {
            for j in requests[r].results[k].writes.indices where requests[r].results[k].writes[j].hash == hash {
                requests[r].results[k].writes[j].task = task
            }
        }
    }
}

extension DemoTasks {
    /// Sets the worker and the Work item of a record `create` made.
    mutating func attach(_ id: String, worker: String, work: String?) -> TaskRecord {
        guard let i = records.firstIndex(where: { $0.id == id }) else { preconditionFailure("attached an unknown task") }
        records[i].worker = worker
        records[i].work = work
        return records[i]
    }
}

// MARK: Fixtures

enum DemoWrites {
    static let writer = WriterInfo(
        worker: "publisher", description: "Posts approved messages to Slack and comments on Linear. It reads no web pages.")

    /// Markdown and a link, shown as plain text: no bold, no live link.
    static let post = write(
        "Post to #finance", "slack:#finance",
        "Provider B wins for our EU invoices.\n\n**Why:** it sends all 27 EU formats; A covers 19.\n"
            + "- Sources: [EU formats](https://example.com/eu-invoicing)\n",
        reason: "Finance asked for the decision in #finance.")

    static let comment = write(
        "Comment on FIN-12", "linear:FIN-12", "Decision: Provider B. The memo follows this week.",
        reason: "FIN-12 tracks the provider decision.")

    /// The UI tests' 8 KiB content: 204 numbered lines of 40 bytes.
    static let longComment = write(
        "Comment on FIN-12", "linear:FIN-12",
        (1...204).map { String(format: "Line %03d: keep the old invoice numbers.\n", $0) }.joined(),
        reason: "FIN-12 tracks the billing migration.")

    private static func write(_ action: String, _ target: String, _ content: String, reason: String) -> ResultWrite {
        ResultWrite(action: action, target: target, content: content,
                    hash: DemoWork.hash(action: action, target: target, content: content), reason: reason)
    }
}

extension DemoWork {
    /// The UI tests' `-demo-writes`: `-demo-staff` with `publisher` as `work`'s writer.
    /// The researcher's result of q-0000d002 proposes i-0000f005 "Post to #finance";
    /// q-0000d001's version 1 result, older than its version 2, proposes i-0000f006, an
    /// 8 KiB comment.
    static var testsWriting: DemoWork {
        testsStaffed.writing(request: "q-0000d002", items: ("i-0000f005", "i-0000f006"),
                             older: ("q-0000d001", 1), long: true)
    }

    static var showcaseWriting: DemoWork {
        showcaseStaffed.writing(request: "q-5c00d002", items: ("i-5c00f005", "i-5c00f006"), older: nil, long: false)
    }
}
