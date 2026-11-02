import Foundation
import MuxaProtocol

/// What the Start loop and New Task sheets edit for a Task Loop (ADR 0019), against
/// the workspace's allowed `checks`. Checked here for quick feedback; the host has
/// the final say. The Android app checks the same.
public struct LoopDraft: Equatable, Sendable {
    /// The workspace's `checks`; nil means the workspace has no loops.
    public var checks: WorkspaceChecks?
    public var check: String
    public var maxAttempts: Int

    public static let defaultMaxAttempts = 3
    public static let attempts = 1...10
    private static let maxCommandBytes = 4096

    /// Shown where a workspace has no `[workspace.loop]` table.
    public static let noChecks =
        "This workspace has no checks, so it has no loops. The host allows them in workspaces.toml, under [workspace.loop]."

    /// Starts at the workspace's first listed check, or empty for `any`.
    public init(checks: WorkspaceChecks?, check: String? = nil, maxAttempts: Int = defaultMaxAttempts) {
        self.checks = checks
        if let check {
            self.check = check
        } else if case .list(let commands) = checks {
            self.check = commands.first ?? ""
        } else {
            self.check = ""
        }
        self.maxAttempts = maxAttempts
    }

    /// What to fix before the loop can start; nil when it can.
    public var problem: String? {
        switch checks {
        case nil:
            return Self.noChecks
        case .list(let commands):
            if !commands.contains(check) { return "Pick a check." }
        case .any:
            let command = check.trimmingCharacters(in: .whitespacesAndNewlines)
            if command.isEmpty { return "Enter a check command." }
            if command.utf8.count > Self.maxCommandBytes { return "The check command is too long." }
            if command.unicodeScalars.contains(where: Self.isControl) {
                return "The check command cannot contain control characters."
            }
        }
        if !Self.attempts.contains(maxAttempts) { return "Max attempts must be 1 to 10." }
        return nil
    }

    /// The `loop` of the request; nil while there is a problem. A listed check is sent exactly as listed.
    public var spec: LoopSpec? {
        guard problem == nil else { return nil }
        let command: String
        if case .list = checks {
            command = check
        } else {
            command = check.trimmingCharacters(in: .whitespacesAndNewlines)
        }
        return LoopSpec(check: command, maxAttempts: UInt32(maxAttempts))
    }

    /// C0 and C1 controls, and DEL.
    private static func isControl(_ s: Unicode.Scalar) -> Bool {
        s.value < 0x20 || (0x7F...0x9F).contains(s.value)
    }
}
