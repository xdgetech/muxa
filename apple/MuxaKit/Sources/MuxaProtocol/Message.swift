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
    /// A code added by a newer bridge.
    case other(String)
}

extension ErrorCode: Codable {
    private static let names: [(ErrorCode, String)] = [
        (.badRequest, "bad-request"), (.invalidPane, "invalid-pane"), (.noPane, "no-pane"),
        (.unsupportedKey, "unsupported-key"), (.tooLarge, "too-large"), (.tmux, "tmux"),
        (.internal, "internal"), (.unsupported, "unsupported"),
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
        case watch, pane, cols, rows, data, reason, chat, doc, path, text, mtime
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
