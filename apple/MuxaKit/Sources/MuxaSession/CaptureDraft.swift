import Foundation
import MuxaProtocol

/// What the Capture sheet edits (ADR 0046). Checked here for quick feedback; the host
/// checks again and has the final say.
public struct CaptureDraft: Equatable, Sendable {
    /// Made once per capture, so sending it again never makes a second one.
    public let clientId: String
    public var text: String
    /// Nil: no workspace yet.
    public var workspace: String?

    /// The host keeps at most 16 KiB of text.
    public static let maxBytes = 16 * 1024
    /// The title a Task gets from the first line, as muxa for tmux cuts it.
    public static let titleLength = 72

    public init(clientId: String = UUID().uuidString, text: String = "", workspace: String? = nil) {
        self.clientId = clientId
        self.text = text
        self.workspace = workspace
    }

    /// The text as sent: without leading and trailing blank space.
    public var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    /// What to fix before the capture can be saved; nil when it can.
    public var problem: String? {
        let text = trimmed
        if text.isEmpty { return "Write something first." }
        if text.utf8.count > Self.maxBytes { return "This is too long. Keep it under 16 KB." }
        return nil
    }

    /// "Start a Task instead…": the first line as the title, the whole text as the goal.
    public func taskDraft(_ base: TaskRecordDraft) -> TaskRecordDraft {
        var draft = base
        let text = trimmed
        guard !text.isEmpty else { return draft }
        draft.title = Self.firstLine(text, limit: Self.titleLength)
        draft.goal = text
        return draft
    }

    /// The first non-blank line, cut to `limit` characters.
    public static func firstLine(_ text: String, limit: Int) -> String {
        let line = text.split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty } ?? ""
        return String(line.prefix(limit))
    }
}

extension PaneSession {
    /// The request never reached the host, or its answer never came back: the
    /// connection was down. Anything the bridge refused is not this.
    public nonisolated static func isConnectionLoss(_ error: any Error) -> Bool {
        if case Failure.notConnected = error { return true }
        return false
    }
}
