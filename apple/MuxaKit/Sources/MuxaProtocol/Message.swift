import Foundation

/// Changes when a message changes incompatibly.
public let protocolVersion: UInt32 = 1

/// Sent by the bridge.
public enum Message: Equatable, Sendable {
    /// The first line the bridge sends, before any request.
    case hello(protocol: UInt32, bridge: String, tmux: String, capabilities: [String])
    /// The answer to one request. `id` is nil only when the request was unreadable.
    case result(id: UInt64?, error: ErrorBody?, sessions: [Session]?)
    /// Reset the emulator to `cols` x `rows`, then feed `data`.
    case screen(watch: UInt64, pane: String, cols: UInt16, rows: UInt16, data: Data)
    /// Feed `data` to the emulator of the current screen.
    case output(watch: UInt64, pane: String, data: Data)
    /// A pane's scrollback, then its screen: lines `cols` wide, each ending in `\n`, with
    /// colors as escape sequences (capability `history`). For a separate emulator.
    case history(history: UInt64, pane: String, cols: UInt16, data: Data, truncated: Bool)
    /// Sessions, windows or panes changed: send `list` again.
    case sessionsChanged
    /// The watched pane no longer exists.
    case paneGone(watch: UInt64, pane: String, reason: String?)
    /// New or changed entries of a chat; see `ChatUpdate`.
    case chatEntries(ChatUpdate)
    /// The chat stopped, e.g. the agent exited.
    case chatEnded(chat: UInt64, reason: String)
    /// The documents for a `docs` request, just before its `result`.
    case docList(DocListing)
    /// The whole text of the open document, first and after every change.
    case docContent(doc: UInt64, path: String, text: String, mtime: UInt64?)
    /// The open document is no longer readable.
    case docEnded(doc: UInt64, reason: String)
    /// The changed files for a `changes` request, just before its `result`.
    case changeList(ChangeListing)
    /// The unified diff of the open file, first and after every change; nil text for a binary file.
    case diffContent(diff: UInt64, path: String, text: String?, binary: Bool)
    /// The open diff stopped, e.g. the file has no changes any more.
    case diffEnded(diff: UInt64, reason: String)
    /// The images for an `images` request, just before its `result`.
    case imageList(ImageListing)
    /// The whole file of the open image, first and after every change.
    case imageContent(image: UInt64, path: String, data: Data, mtime: UInt64?)
    /// The open image is no longer readable.
    case imageEnded(image: UInt64, reason: String)
    /// The session a `new-task` made or found, just before its `result`.
    case taskCreated(task: UInt64, session: String)
    /// The task records for a `list-tasks` request, just before its `result`.
    case taskList(TaskListing)
    /// The record a task request wrote, just before its `result`.
    case taskSaved(task: UInt64, record: TaskRecord)
    /// A task record changed on the host: send `list-tasks` again (capability
    /// `tasks-changed`).
    case tasksChanged
    /// The Routines for a `list-routines` request, just before its `result`.
    case routineList(routines: UInt64, entries: [RoutineInfo], problems: [RoutineProblem] = [])
    /// The Routine a `run-routine` started, just before its `result`.
    case routineSaved(routine: UInt64, entry: RoutineInfo)
    /// The stored Inbox entries for a `list-inbox` request, newest first. Level 3
    /// also sends `items`: every item, Routine entries included, in the order to show them.
    /// Level 8 also sends `work` (open first, then the ones moved down with Later, then
    /// at most 100 done or withdrawn), the unsorted `captures`, newest first, and the
    /// `requests` of the listed Work items (ADR 0046).
    /// Level 9 also sends the kept `plans`, newest first, and `assistant`: the overall
    /// assistant and the delegates (ADR 0047).
    /// Level 10 also sends `policies` (active, then expired), `hypotheses` (`proposed` and
    /// `rejected`, newest first) and at most 50 `corrections`, newest first (ADR 0048).
    case inboxList(
        inbox: UInt64, entries: [InboxEntry], items: [InboxItem] = [], work: [WorkItem] = [],
        captures: [Capture] = [], requests: [AssistantRequest] = [], plans: [Plan] = [],
        assistant: AssistantInfo? = nil, policies: [Policy] = [], hypotheses: [Hypothesis] = [],
        corrections: [Correction] = [])
    /// What an `update-inbox` wrote, just before its `result`: the Routine entry,
    /// the item, or both for a Routine's item.
    case inboxSaved(inbox: UInt64, entry: InboxEntry?, item: InboxItem? = nil)
    /// The session a `start-session` made, just before its `result`. `start`
    /// is the request id, `session` the `$n` id.
    case sessionStarted(start: UInt64, session: String, name: String)
    /// What a `capture` or `sort-capture` wrote, just before its `result`: the Capture,
    /// and the Work item and request it made, if any.
    case captureSaved(capture: UInt64, entry: Capture, work: WorkItem? = nil, request: AssistantRequest? = nil)
    /// What an `update-work`, `request-assistant` or `cancel-request` wrote, just before
    /// its `result`.
    case workSaved(work: UInt64, entry: WorkItem, request: AssistantRequest? = nil)
    /// The Work item a `get-work` asked for, with its derived state, just before its
    /// `result`: the Captures `entry.captures` names, sorted or not, in that order, and its requests.
    case workDetail(work: UInt64, entry: WorkItem, captures: [Capture] = [], requests: [AssistantRequest] = [])
    /// What a `plan-now` or `apply-notes` wrote, just before its `result`.
    case planSaved(plan: UInt64, entry: Plan)
    /// What an `add-policy`, an `update-policy edit` or a `confirm` wrote, just before its
    /// `result` (`work` level 10).
    case policySaved(policy: UInt64, entry: Policy)
    /// The bridge is exiting.
    case bye(reason: String)
    /// A type this client does not know. The protocol says to ignore it.
    case unknown(type: String)
}

public struct ErrorBody: Codable, Equatable, Sendable {
    public var code: ErrorCode
    public var message: String

    public init(code: ErrorCode, message: String) {
        self.code = code
        self.message = message
    }
}

public enum ErrorCode: Equatable, Sendable {
    case badRequest, invalidPane, noPane, unsupportedKey, tooLarge, tmux, `internal`
    /// No agent adapter knows the program in the pane.
    case unsupported
    /// The task launcher failed; the message says why.
    case taskFailed
    /// The task record changed since the client read it; list it again.
    case conflict
    /// Ending the Task would remove uncommitted or unmerged work; the message says
    /// which. Nothing changed (ADR 0043).
    case wouldLoseWork
    /// A code added by a newer bridge.
    case other(String)
}

extension ErrorCode: Codable {
    private static let names: [(ErrorCode, String)] = [
        (.badRequest, "bad-request"), (.invalidPane, "invalid-pane"), (.noPane, "no-pane"),
        (.unsupportedKey, "unsupported-key"), (.tooLarge, "too-large"), (.tmux, "tmux"),
        (.internal, "internal"), (.unsupported, "unsupported"), (.taskFailed, "task-failed"),
        (.conflict, "conflict"), (.wouldLoseWork, "would-lose-work"),
    ]

    public var name: String {
        if case .other(let name) = self { return name }
        return Self.names.first { $0.0 == self }?.1 ?? "internal"
    }

    public init(from decoder: any Decoder) throws {
        let name = try decoder.singleValueContainer().decode(String.self)
        self = Self.names.first { $0.1 == name }?.0 ?? .other(name)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(name)
    }
}

extension Message: Codable {
    private enum Key: String, CodingKey {
        case type, `protocol`, bridge, tmux, capabilities, id, error, sessions
        case watch, pane, cols, rows, data, reason, chat, doc, path, text, mtime, task, session, diff, binary, image, record
        case routines, routine, entries, entry, problems, inbox, items, item
        case start, name
        case work, captures, requests, capture, request, plans, assistant, plan
        case policies, hypotheses, corrections, policy
        case history, truncated
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        let type = try c.decode(String.self, forKey: .type)
        switch type {
        case "hello":
            self = .hello(
                protocol: try c.decode(UInt32.self, forKey: .protocol),
                bridge: try c.decode(String.self, forKey: .bridge),
                tmux: try c.decode(String.self, forKey: .tmux),
                capabilities: try c.decode([String].self, forKey: .capabilities))
        case "result":
            self = .result(
                id: try c.decodeIfPresent(UInt64.self, forKey: .id),
                error: try c.decodeIfPresent(ErrorBody.self, forKey: .error),
                sessions: try c.decodeIfPresent([Session].self, forKey: .sessions))
        case "screen":
            self = .screen(
                watch: try c.decode(UInt64.self, forKey: .watch),
                pane: try c.decode(String.self, forKey: .pane),
                cols: try c.decode(UInt16.self, forKey: .cols),
                rows: try c.decode(UInt16.self, forKey: .rows),
                data: try c.decodeBase64(forKey: .data))
        case "history":
            self = .history(
                history: try c.decode(UInt64.self, forKey: .history),
                pane: try c.decode(String.self, forKey: .pane),
                cols: try c.decode(UInt16.self, forKey: .cols),
                data: try c.decodeBase64(forKey: .data),
                truncated: try c.decodeIfPresent(Bool.self, forKey: .truncated) ?? false)
        case "output":
            self = .output(
                watch: try c.decode(UInt64.self, forKey: .watch),
                pane: try c.decode(String.self, forKey: .pane),
                data: try c.decodeBase64(forKey: .data))
        case "sessions-changed":
            self = .sessionsChanged
        case "pane-gone":
            self = .paneGone(
                watch: try c.decode(UInt64.self, forKey: .watch),
                pane: try c.decode(String.self, forKey: .pane),
                reason: try c.decodeIfPresent(String.self, forKey: .reason))
        case "chat-entries":
            self = .chatEntries(try ChatUpdate(from: decoder))
        case "chat-ended":
            self = .chatEnded(
                chat: try c.decode(UInt64.self, forKey: .chat),
                reason: try c.decode(String.self, forKey: .reason))
        case "doc-list":
            self = .docList(try DocListing(from: decoder))
        case "doc-content":
            self = .docContent(
                doc: try c.decode(UInt64.self, forKey: .doc),
                path: try c.decode(String.self, forKey: .path),
                text: try c.decode(String.self, forKey: .text),
                mtime: try c.decodeIfPresent(UInt64.self, forKey: .mtime))
        case "doc-ended":
            self = .docEnded(
                doc: try c.decode(UInt64.self, forKey: .doc),
                reason: try c.decode(String.self, forKey: .reason))
        case "change-list":
            self = .changeList(try ChangeListing(from: decoder))
        case "diff-content":
            self = .diffContent(
                diff: try c.decode(UInt64.self, forKey: .diff),
                path: try c.decode(String.self, forKey: .path),
                text: try c.decodeIfPresent(String.self, forKey: .text),
                binary: try c.decodeIfPresent(Bool.self, forKey: .binary) ?? false)
        case "diff-ended":
            self = .diffEnded(
                diff: try c.decode(UInt64.self, forKey: .diff),
                reason: try c.decode(String.self, forKey: .reason))
        case "image-list":
            self = .imageList(try ImageListing(from: decoder))
        case "image-content":
            self = .imageContent(
                image: try c.decode(UInt64.self, forKey: .image),
                path: try c.decode(String.self, forKey: .path),
                data: try c.decodeBase64(forKey: .data),
                mtime: try c.decodeIfPresent(UInt64.self, forKey: .mtime))
        case "image-ended":
            self = .imageEnded(
                image: try c.decode(UInt64.self, forKey: .image),
                reason: try c.decode(String.self, forKey: .reason))
        case "task-created":
            self = .taskCreated(
                task: try c.decode(UInt64.self, forKey: .task),
                session: try c.decode(String.self, forKey: .session))
        case "task-list":
            self = .taskList(try TaskListing(from: decoder))
        case "task-saved":
            self = .taskSaved(
                task: try c.decode(UInt64.self, forKey: .task),
                record: try c.decode(TaskRecord.self, forKey: .record))
        case "tasks-changed":
            self = .tasksChanged
        case "routine-list":
            self = .routineList(
                routines: try c.decode(UInt64.self, forKey: .routines),
                entries: try c.decode([RoutineInfo].self, forKey: .entries),
                problems: try c.decodeIfPresent([RoutineProblem].self, forKey: .problems) ?? [])
        case "routine-saved":
            self = .routineSaved(
                routine: try c.decode(UInt64.self, forKey: .routine),
                entry: try c.decode(RoutineInfo.self, forKey: .entry))
        case "inbox-list":
            self = .inboxList(
                inbox: try c.decode(UInt64.self, forKey: .inbox),
                entries: try c.decode([InboxEntry].self, forKey: .entries),
                items: try c.decodeIfPresent([InboxItem].self, forKey: .items) ?? [],
                work: try c.decodeIfPresent([WorkItem].self, forKey: .work) ?? [],
                captures: try c.decodeIfPresent([Capture].self, forKey: .captures) ?? [],
                requests: try c.decodeIfPresent([AssistantRequest].self, forKey: .requests) ?? [],
                plans: try c.decodeIfPresent([Plan].self, forKey: .plans) ?? [],
                assistant: try c.decodeIfPresent(AssistantInfo.self, forKey: .assistant),
                policies: try c.decodeIfPresent([Policy].self, forKey: .policies) ?? [],
                hypotheses: try c.decodeIfPresent([Hypothesis].self, forKey: .hypotheses) ?? [],
                corrections: try c.decodeIfPresent([Correction].self, forKey: .corrections) ?? [])
        case "inbox-saved":
            self = .inboxSaved(
                inbox: try c.decode(UInt64.self, forKey: .inbox),
                entry: try c.decodeIfPresent(InboxEntry.self, forKey: .entry),
                item: try c.decodeIfPresent(InboxItem.self, forKey: .item))
        case "session-started":
            self = .sessionStarted(
                start: try c.decode(UInt64.self, forKey: .start),
                session: try c.decode(String.self, forKey: .session),
                name: try c.decode(String.self, forKey: .name))
        case "capture-saved":
            self = .captureSaved(
                capture: try c.decode(UInt64.self, forKey: .capture),
                entry: try c.decode(Capture.self, forKey: .entry),
                work: try c.decodeIfPresent(WorkItem.self, forKey: .work),
                request: try c.decodeIfPresent(AssistantRequest.self, forKey: .request))
        case "work-saved":
            self = .workSaved(
                work: try c.decode(UInt64.self, forKey: .work),
                entry: try c.decode(WorkItem.self, forKey: .entry),
                request: try c.decodeIfPresent(AssistantRequest.self, forKey: .request))
        case "work-detail":
            self = .workDetail(
                work: try c.decode(UInt64.self, forKey: .work),
                entry: try c.decode(WorkItem.self, forKey: .entry),
                captures: try c.decodeIfPresent([Capture].self, forKey: .captures) ?? [],
                requests: try c.decodeIfPresent([AssistantRequest].self, forKey: .requests) ?? [])
        case "plan-saved":
            self = .planSaved(
                plan: try c.decode(UInt64.self, forKey: .plan), entry: try c.decode(Plan.self, forKey: .entry))
        case "policy-saved":
            self = .policySaved(
                policy: try c.decode(UInt64.self, forKey: .policy), entry: try c.decode(Policy.self, forKey: .entry))
        case "bye":
            self = .bye(reason: try c.decode(String.self, forKey: .reason))
        default:
            self = .unknown(type: type)
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        switch self {
        case .hello(let version, let bridge, let tmux, let capabilities):
            try c.encode("hello", forKey: .type)
            try c.encode(version, forKey: .protocol)
            try c.encode(bridge, forKey: .bridge)
            try c.encode(tmux, forKey: .tmux)
            try c.encode(capabilities, forKey: .capabilities)
        case .result(let id, let error, let sessions):
            try c.encode("result", forKey: .type)
            try c.encodeIfPresent(id, forKey: .id)
            try c.encodeIfPresent(error, forKey: .error)
            try c.encodeIfPresent(sessions, forKey: .sessions)
        case .screen(let watch, let pane, let cols, let rows, let data):
            try c.encode("screen", forKey: .type)
            try c.encode(watch, forKey: .watch)
            try c.encode(pane, forKey: .pane)
            try c.encode(cols, forKey: .cols)
            try c.encode(rows, forKey: .rows)
            try c.encode(data.base64EncodedString(), forKey: .data)
        case .history(let history, let pane, let cols, let data, let truncated):
            try c.encode("history", forKey: .type)
            try c.encode(history, forKey: .history)
            try c.encode(pane, forKey: .pane)
            try c.encode(cols, forKey: .cols)
            try c.encode(data.base64EncodedString(), forKey: .data)
            if truncated { try c.encode(true, forKey: .truncated) }
        case .output(let watch, let pane, let data):
            try c.encode("output", forKey: .type)
            try c.encode(watch, forKey: .watch)
            try c.encode(pane, forKey: .pane)
            try c.encode(data.base64EncodedString(), forKey: .data)
        case .sessionsChanged:
            try c.encode("sessions-changed", forKey: .type)
        case .paneGone(let watch, let pane, let reason):
            try c.encode("pane-gone", forKey: .type)
            try c.encode(watch, forKey: .watch)
            try c.encode(pane, forKey: .pane)
            try c.encodeIfPresent(reason, forKey: .reason)
        case .chatEntries(let update):
            try c.encode("chat-entries", forKey: .type)
            try update.encode(to: encoder)
        case .chatEnded(let chat, let reason):
            try c.encode("chat-ended", forKey: .type)
            try c.encode(chat, forKey: .chat)
            try c.encode(reason, forKey: .reason)
        case .docList(let listing):
            try c.encode("doc-list", forKey: .type)
            try listing.encode(to: encoder)
        case .docContent(let doc, let path, let text, let mtime):
            try c.encode("doc-content", forKey: .type)
            try c.encode(doc, forKey: .doc)
            try c.encode(path, forKey: .path)
            try c.encode(text, forKey: .text)
            try c.encodeIfPresent(mtime, forKey: .mtime)
        case .docEnded(let doc, let reason):
            try c.encode("doc-ended", forKey: .type)
            try c.encode(doc, forKey: .doc)
            try c.encode(reason, forKey: .reason)
        case .changeList(let listing):
            try c.encode("change-list", forKey: .type)
            try listing.encode(to: encoder)
        case .diffContent(let diff, let path, let text, let binary):
            try c.encode("diff-content", forKey: .type)
            try c.encode(diff, forKey: .diff)
            try c.encode(path, forKey: .path)
            try c.encodeIfPresent(text, forKey: .text)
            if binary { try c.encode(true, forKey: .binary) }
        case .diffEnded(let diff, let reason):
            try c.encode("diff-ended", forKey: .type)
            try c.encode(diff, forKey: .diff)
            try c.encode(reason, forKey: .reason)
        case .imageList(let listing):
            try c.encode("image-list", forKey: .type)
            try listing.encode(to: encoder)
        case .imageContent(let image, let path, let data, let mtime):
            try c.encode("image-content", forKey: .type)
            try c.encode(image, forKey: .image)
            try c.encode(path, forKey: .path)
            try c.encode(data.base64EncodedString(), forKey: .data)
            try c.encodeIfPresent(mtime, forKey: .mtime)
        case .imageEnded(let image, let reason):
            try c.encode("image-ended", forKey: .type)
            try c.encode(image, forKey: .image)
            try c.encode(reason, forKey: .reason)
        case .taskCreated(let task, let session):
            try c.encode("task-created", forKey: .type)
            try c.encode(task, forKey: .task)
            try c.encode(session, forKey: .session)
        case .taskList(let listing):
            try c.encode("task-list", forKey: .type)
            try listing.encode(to: encoder)
        case .taskSaved(let task, let record):
            try c.encode("task-saved", forKey: .type)
            try c.encode(task, forKey: .task)
            try c.encode(record, forKey: .record)
        case .tasksChanged:
            try c.encode("tasks-changed", forKey: .type)
        case .routineList(let routines, let entries, let problems):
            try c.encode("routine-list", forKey: .type)
            try c.encode(routines, forKey: .routines)
            try c.encode(entries, forKey: .entries)
            if !problems.isEmpty { try c.encode(problems, forKey: .problems) }
        case .routineSaved(let routine, let entry):
            try c.encode("routine-saved", forKey: .type)
            try c.encode(routine, forKey: .routine)
            try c.encode(entry, forKey: .entry)
        case .inboxList(
            let inbox, let entries, let items, let work, let captures, let requests, let plans, let assistant,
            let policies, let hypotheses, let corrections):
            try c.encode("inbox-list", forKey: .type)
            try c.encode(inbox, forKey: .inbox)
            try c.encode(entries, forKey: .entries)
            if !items.isEmpty { try c.encode(items, forKey: .items) }
            if !work.isEmpty { try c.encode(work, forKey: .work) }
            if !captures.isEmpty { try c.encode(captures, forKey: .captures) }
            if !requests.isEmpty { try c.encode(requests, forKey: .requests) }
            if !plans.isEmpty { try c.encode(plans, forKey: .plans) }
            try c.encodeIfPresent(assistant, forKey: .assistant)
            if !policies.isEmpty { try c.encode(policies, forKey: .policies) }
            if !hypotheses.isEmpty { try c.encode(hypotheses, forKey: .hypotheses) }
            if !corrections.isEmpty { try c.encode(corrections, forKey: .corrections) }
        case .inboxSaved(let inbox, let entry, let item):
            try c.encode("inbox-saved", forKey: .type)
            try c.encode(inbox, forKey: .inbox)
            try c.encodeIfPresent(entry, forKey: .entry)
            try c.encodeIfPresent(item, forKey: .item)
        case .sessionStarted(let start, let session, let name):
            try c.encode("session-started", forKey: .type)
            try c.encode(start, forKey: .start)
            try c.encode(session, forKey: .session)
            try c.encode(name, forKey: .name)
        case .captureSaved(let capture, let entry, let work, let request):
            try c.encode("capture-saved", forKey: .type)
            try c.encode(capture, forKey: .capture)
            try c.encode(entry, forKey: .entry)
            try c.encodeIfPresent(work, forKey: .work)
            try c.encodeIfPresent(request, forKey: .request)
        case .workSaved(let work, let entry, let request):
            try c.encode("work-saved", forKey: .type)
            try c.encode(work, forKey: .work)
            try c.encode(entry, forKey: .entry)
            try c.encodeIfPresent(request, forKey: .request)
        case .workDetail(let work, let entry, let captures, let requests):
            try c.encode("work-detail", forKey: .type)
            try c.encode(work, forKey: .work)
            try c.encode(entry, forKey: .entry)
            if !captures.isEmpty { try c.encode(captures, forKey: .captures) }
            if !requests.isEmpty { try c.encode(requests, forKey: .requests) }
        case .planSaved(let plan, let entry):
            try c.encode("plan-saved", forKey: .type)
            try c.encode(plan, forKey: .plan)
            try c.encode(entry, forKey: .entry)
        case .policySaved(let policy, let entry):
            try c.encode("policy-saved", forKey: .type)
            try c.encode(policy, forKey: .policy)
            try c.encode(entry, forKey: .entry)
        case .bye(let reason):
            try c.encode("bye", forKey: .type)
            try c.encode(reason, forKey: .reason)
        case .unknown(let type):
            try c.encode(type, forKey: .type)
        }
    }
}

extension KeyedDecodingContainer {
    func decodeBase64(forKey key: K) throws -> Data {
        let text = try decode(String.self, forKey: key)
        guard let data = Data(base64Encoded: text) else {
            throw DecodingError.dataCorruptedError(
                forKey: key, in: self, debugDescription: "invalid base64")
        }
        return data
    }
}
