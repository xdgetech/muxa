import Foundation

/// Sent by the client. Every request has an `id`, echoed in its `result`.
public enum Request: Equatable, Sendable {
    case ping(id: UInt64)
    case list(id: UInt64)
    /// Show this pane. `screen` and `output` for it carry this request's id as `watch`.
    case watch(id: UInt64, pane: String)
    case unwatch(id: UInt64)
    /// Named keys, e.g. `C-c`, `Up`, `S-Enter`.
    case keys(id: UInt64, pane: String, keys: [String])
    /// Typed literally.
    case text(id: UInt64, pane: String, text: String)
    /// Raw key bytes, for a client that encodes keys itself.
    case bytes(id: UInt64, pane: String, data: Data)
    /// Pasted, bracketed when the program asked for it.
    case paste(id: UInt64, pane: String, text: String)
    /// Follow the conversation of the agent in this pane. Needs the `chat`
    /// capability. `chat-entries` for it carry this request's id as `chat`.
    case chat(id: UInt64, pane: String)
    case unchat(id: UInt64)
    /// This pane's Markdown documents (capability `docs`); a `doc-list` comes first.
    case docs(id: UInt64, pane: String)
    /// Open a listed document and follow its changes; `doc-content` carries this id as `doc`.
    case doc(id: UInt64, pane: String, path: String)
    case undoc(id: UInt64)
    /// The client is leaving; the bridge exits.
    case bye(id: UInt64)

    public var id: UInt64 {
        switch self {
        case .ping(let id), .list(let id), .unwatch(let id), .unchat(let id), .undoc(let id), .bye(let id), .docs(let id, _), .doc(let id, _, _),
            .watch(let id, _), .chat(let id, _), .keys(let id, _, _), .text(let id, _, _),
            .bytes(let id, _, _), .paste(let id, _, _):
            id
        }
    }
}

extension Request: Codable {
    fileprivate enum Key: String, CodingKey {
        case type, id, pane, keys, text, data, path
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Key.self)
        switch self {
        case .ping(let id): try c.head("ping", id)
        case .list(let id): try c.head("list", id)
        case .unwatch(let id): try c.head("unwatch", id)
        case .bye(let id): try c.head("bye", id)
        case .unchat(let id): try c.head("unchat", id)
        case .undoc(let id): try c.head("undoc", id)
        case .docs(let id, let pane):
            try c.head("docs", id)
            try c.encode(pane, forKey: .pane)
        case .doc(let id, let pane, let path):
            try c.head("doc", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(path, forKey: .path)
        case .watch(let id, let pane):
            try c.head("watch", id)
            try c.encode(pane, forKey: .pane)
        case .chat(let id, let pane):
            try c.head("chat", id)
            try c.encode(pane, forKey: .pane)
        case .keys(let id, let pane, let keys):
            try c.head("keys", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(keys, forKey: .keys)
        case .text(let id, let pane, let text):
            try c.head("text", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(text, forKey: .text)
        case .bytes(let id, let pane, let data):
            try c.head("bytes", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(data.base64EncodedString(), forKey: .data)
        case .paste(let id, let pane, let text):
            try c.head("paste", id)
            try c.encode(pane, forKey: .pane)
            try c.encode(text, forKey: .text)
        }
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Key.self)
        let type = try c.decode(String.self, forKey: .type)
        let id = try c.decode(UInt64.self, forKey: .id)
        func pane() throws -> String { try c.decode(String.self, forKey: .pane) }
        switch type {
        case "ping": self = .ping(id: id)
        case "list": self = .list(id: id)
        case "unwatch": self = .unwatch(id: id)
        case "bye": self = .bye(id: id)
        case "watch": self = .watch(id: id, pane: try pane())
        case "chat": self = .chat(id: id, pane: try pane())
        case "unchat": self = .unchat(id: id)
        case "undoc": self = .undoc(id: id)
        case "docs": self = .docs(id: id, pane: try pane())
        case "doc": self = .doc(id: id, pane: try pane(), path: try c.decode(String.self, forKey: .path))
        case "keys": self = .keys(id: id, pane: try pane(), keys: try c.decode([String].self, forKey: .keys))
        case "text": self = .text(id: id, pane: try pane(), text: try c.decode(String.self, forKey: .text))
        case "paste": self = .paste(id: id, pane: try pane(), text: try c.decode(String.self, forKey: .text))
        case "bytes": self = .bytes(id: id, pane: try pane(), data: try c.decodeBase64(forKey: .data))
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: c, debugDescription: "unknown request type \(type)")
        }
    }
}

extension KeyedEncodingContainer<Request.Key> {
    fileprivate mutating func head(_ type: String, _ id: UInt64) throws {
        try encode(type, forKey: .type)
        try encode(id, forKey: .id)
    }
}
