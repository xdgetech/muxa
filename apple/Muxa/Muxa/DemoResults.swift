import Foundation
import MuxaProtocol

/// The demo's `result` items (`work` level 16, ADR 0058), with the host's checks: every stored
/// Result makes one item; approve starts its hand-off once; reply asks the member who wrote it
/// again, as a new version; on a `write` item reply is Revise.
extension DemoWork {
    private static var now: UInt64 { UInt64(Date().timeIntervalSince1970) }

    /// Moves the runs on by their age. Each Result stored makes its `result` item at level 16.
    /// Returns the runs that ended.
    mutating func advanceRuns() -> [DemoRun] {
        let live = Set(assistant.runs.keys)
        let ended = assistant.advance()
        guard reviewsResults else { return ended }
        for id in live where assistant.runs[id] == nil {
            guard let request = assistant.requests.first(where: { $0.id == id }), request.delivery == .resulted,
                  let result = request.results.first
            else { continue }
            resultItemCount += 1
            let title = assistant.work.first { $0.id == request.work }?.title ?? request.work
            taskItems.insert(Self.resultItem(String(format: "i-%08x", 0x700 + resultItemCount), request: request,
                                             result: result, title: title), at: 0)
        }
        return ended
    }

    /// Send on a `result` item, Revise on a `write` item: a new version of the request whose
    /// step 1 is the member who wrote the Result, asked by the user, with the text as its
    /// purpose. The item is answered `reply`.
    mutating func followUp(
        _ id: String, text: String?
    ) throws(Refusal) -> (WorkItem, AssistantRequest, DemoAssistant.RunChange, InboxItem) {
        guard let i = taskItems.firstIndex(where: { $0.id == id }) else { throw Refusal(.badRequest, "\(id) is not an item") }
        let item = taskItems[i]
        guard item.state == .open, item.answers.contains(.reply) else {
            throw Refusal(.badRequest, "reply is not an answer of \(id)")
        }
        guard let text, !text.isEmpty else { throw Refusal(.badRequest, "reply needs a text") }
        guard let asked = item.result?.request ?? item.write?.request,
              let q = assistant.requests.first(where: { $0.id == asked }),
              let w = assistant.work.first(where: { $0.id == q.work })
        else { throw Refusal(.badRequest, "\(id) names no request") }
        let (entry, request, change) = try assistant.ask(
            w.id, rev: w.rev, scope: q.scope, workspace: q.workspace, purpose: text,
            worker: item.result?.worker ?? item.write?.worker)
        taskItems[i].state = .answered
        taskItems[i].answer = .reply
        taskItems[i].answeredAt = Self.now
        taskItems[i].answers = []
        resolveReplacedResults()
        return (entry, request, change, taskItems[i])
    }

    /// A newer version of its request resolves an open `result` item. A `write` item stays open.
    mutating func resolveReplacedResults() {
        for i in taskItems.indices where taskItems[i].kind == .result && taskItems[i].state == .open {
            guard let r = taskItems[i].result,
                  let current = assistant.requests.first(where: { $0.id == r.request }), current.version > r.version
            else { continue }
            taskItems[i].state = .resolved
            taskItems[i].answers = []
        }
    }

    /// As a level 15 host keeps it: a Result's hand-off is a `handoff` item, a Result without
    /// one makes no item, and a `write` item has no Revise.
    mutating func forgetResultReview() {
        taskItems = taskItems.compactMap { item in
            switch item.kind {
            case .result:
                guard let h = item.result?.handoffProposal else { return nil }
                return InboxItem(
                    id: item.id, kind: .handoff, state: item.state,
                    question: "\(h.from) asks \(h.worker) to \(h.purpose.prefix(1).lowercased() + h.purpose.dropFirst())",
                    workspace: item.workspace, title: item.title, createdAt: item.createdAt, handoff: h,
                    answers: item.answers.contains(.approve) ? [.approve, .reject, .later] : [])
            case .write:
                var kept = item
                kept.answers.removeAll { $0 == .reply }
                return kept
            default:
                return item
            }
        }
    }

    /// The item the host makes for a stored Result: the summary, the findings' first lines, the
    /// open questions, the next steps, the files and the hand-off the step proposes.
    static func resultItem(_ id: String, request: AssistantRequest, result: AssistResult, title: String) -> InboxItem {
        let step = result.step ?? 1
        let handoff = request.steps.first { $0.step == step }?.handoff
        let who = result.worker ?? "The assistant"
        let question = handoff.map { "\(who) asks \($0.worker) to: \($0.purpose)" }
            ?? "\(who) finished: \(firstLine(result.summary))"
        var seen = Set<String>()
        let links = result.findings.flatMap(\.sources).compactMap { s -> SourceLink? in
            guard let url = s.url, seen.insert(url).inserted else { return nil }
            return SourceLink(url: url, title: s.title)
        }
        return InboxItem(
            id: id, kind: .result, state: .open, question: question, workspace: request.workspace, title: title,
            createdAt: result.at, summary: result.summary, attachments: Attachments(links: Array(links.prefix(10))),
            result: ResultReview(
                request: request.id, version: result.version, step: step, work: request.work, worker: result.worker,
                findings: result.findings.map { String(firstLine($0.text).prefix(200)) },
                openQuestions: result.openQuestions, next: result.next, files: result.files, handoff: handoff),
            answers: (handoff == nil ? [] : [.approve]) + [.reply, .dismiss, .later])
    }

    private static func firstLine(_ text: String) -> String {
        text.split(whereSeparator: \.isNewline).first.map(String.init) ?? text
    }
}

extension ResultReview {
    /// What approve starts, as the `handoff` item of a level 15 host carries it.
    var handoffProposal: HandoffProposal? {
        handoff.map {
            HandoffProposal(request: request, version: version, step: step, work: work, from: worker ?? "the assistant",
                            worker: $0.worker, purpose: $0.purpose)
        }
    }
}

extension DemoBridge {
    /// At level 16, reply on a `result` or `write` item asks the member again.
    func asksAgain(_ id: String) -> Bool {
        guard Self.workLevel >= workResultReview else { return false }
        return lock.withLock { work.taskItems.contains { $0.id == id && ($0.kind == .result || $0.kind == .write) } }
    }

    /// Send or Revise: `work-saved` with the new version, then `inbox-saved`, then the result.
    /// A refusal leaves the item open.
    func followUp(id: UInt64, item: String, text: String?) {
        let outcome = lock.withLock {
            () -> Result<(WorkItem, AssistantRequest, DemoAssistant.RunChange, InboxItem), DemoWork.Refusal> in
            do throws(DemoWork.Refusal) { return .success(try work.followUp(item, text: text)) } catch { return .failure(error) }
        }
        switch outcome {
        case .success(let (entry, request, change, saved)):
            emit(.workSaved(work: id, entry: entry, request: request))
            emit(.inboxSaved(inbox: id, entry: nil, item: saved))
            ok(id)
            apply(change)
        case .failure(let refusal):
            emit(.result(id: id, error: refusal.body, sessions: nil))
        }
    }
}
