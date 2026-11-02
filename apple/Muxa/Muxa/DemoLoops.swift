import Foundation
import MuxaProtocol

// The demo's Task Loop, as the host's daemon answers it (ADR 0019). It never runs a
// check. The Android demo answers the same.

extension DemoTasks {
    /// Where the daemon writes the last check's result, beside the brief.
    static let checkFile = ".muxa/CHECK.md"

    private static let defaultMaxAttempts: UInt32 = 3
    private static let defaultTimeout: UInt64 = 600

    /// Why the host refuses `spec` in `workspace`, as `bad-request`; nil when it allows it.
    static func loopRefusal(_ workspace: WorkspaceInfo, _ spec: LoopSpec) -> String? {
        let maxAttempts = spec.maxAttempts ?? defaultMaxAttempts
        if !(1...10).contains(maxAttempts) { return "maxAttempts must be 1 to 10" }
        switch workspace.checks {
        case nil:
            return "The workspace \(workspace.name) has no [workspace.loop] table in workspaces.toml, so it has no loops."
        case .list(let commands):
            return commands.contains(spec.check) ? nil : "The workspace \(workspace.name) does not allow this check."
        case .any:
            let bad = spec.check.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || spec.check.utf8.count > 4096
                || spec.check.unicodeScalars.contains { $0.value < 0x20 || (0x7F...0x9F).contains($0.value) }
            return bad ? "The check must be at most 4096 bytes, with no control character." : nil
        }
    }

    /// A loop that starts now; the attempts go on from `previous` unless `reset`.
    static func started(_ previous: TaskLoop?, _ spec: LoopSpec, reset: Bool, now: UInt64) -> TaskLoop {
        TaskLoop(check: spec.check, maxAttempts: spec.maxAttempts ?? defaultMaxAttempts,
                 timeout: spec.timeout ?? defaultTimeout, state: .armed,
                 attempts: reset ? 0 : previous?.attempts ?? 0, armedAt: now, since: now,
                 lastCheck: previous?.lastCheck)
    }

    /// Starts a task's loop, also a stopped or passed one, when its workspace allows the check.
    mutating func startLoop(_ id: String, spec: LoopSpec, reset: Bool) throws(Refusal) -> TaskRecord {
        let workspaces = workspaces
        return try changeLoop(id) { (r: TaskRecord, now: UInt64) throws(Refusal) -> TaskLoop in
            guard let workspace = workspaces.first(where: { $0.name == r.workspace }) else {
                throw .badRequest("not a workspace")
            }
            if let why = Self.loopRefusal(workspace, spec) { throw .badRequest(why) }
            return Self.started(r.loop, spec, reset: reset, now: now)
        }
    }

    mutating func stopLoop(_ id: String) throws(Refusal) -> TaskRecord {
        try changeLoop(id) { (r: TaskRecord, now: UInt64) throws(Refusal) -> TaskLoop in
            guard var loop = r.loop else { throw .badRequest("\(id) has no loop") }
            loop.state = .stopped
            loop.reason = .user
            loop.since = now
            return loop
        }
    }

    /// Saves the loop `change` makes, with a new rev.
    private mutating func changeLoop(
        _ id: String, _ change: (TaskRecord, UInt64) throws(Refusal) -> TaskLoop
    ) throws(Refusal) -> TaskRecord {
        guard let i = records.firstIndex(where: { $0.id == id }) else { throw .badRequest("\(id) is not a task") }
        let now = UInt64(Date().timeIntervalSince1970)
        let loop = try change(records[i], now)
        records[i].loop = loop
        records[i].rev += 1
        records[i].updatedAt = now
        return records[i]
    }

    /// What the daemon would write into `CHECK.md`: the attempt, the exit code, the
    /// duration and the log's tail. The Android demo writes the same.
    func checkFile(for id: String?) -> (text: String, mtime: UInt64)? {
        guard let check = record(id)?.loop?.lastCheck else { return nil }
        var text = "# Check\n\n"
        text += "- Attempt: \(check.attempt)\n"
        text += "- Exit: \(check.exit.map(String.init) ?? "timed out")\n"
        text += "- Duration: \(check.seconds) s\n\n"
        text += "## Output (last 200 lines)\n\n```\n"
        switch check.exit {
        case 0:
            text += "test result: ok. 128 passed; 0 failed\n"
        case nil:
            text += "running 128 tests\ntest search::rebuild_index has been running for over 60 seconds\n"
        default:
            text += "running 128 tests\n"
            text += "test auth::passkey_round_trip ... FAILED\n\n"
            text += "failures:\n\n---- auth::passkey_round_trip stdout ----\n"
            text += "assertion failed: response.status == 302\n\n"
            text += "test result: FAILED. 127 passed; 1 failed\n"
        }
        return (text + "```\n", check.at)
    }
}

extension TaskLoop {
    /// The loop after the user set `status`: anything but `open` stops an active loop.
    func after(_ status: TaskStatus, now: UInt64) -> TaskLoop {
        let active = state == .armed || state == .checking || state == .paused
        guard active, status != .open else { return self }
        var stopped = self
        stopped.state = .stopped
        stopped.reason = .status
        stopped.since = now
        return stopped
    }
}
