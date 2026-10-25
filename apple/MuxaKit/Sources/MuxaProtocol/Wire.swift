import Foundation

/// Encodes and decodes protocol lines: one JSON object, no trailing newline.
public enum Wire {
    public static func encode(_ request: Request) throws -> Data {
        try encoder().encode(request)
    }

    public static func encode(_ message: Message) throws -> Data {
        try encoder().encode(message)
    }

    public static func decodeMessage(_ line: Data) throws -> Message {
        try JSONDecoder().decode(Message.self, from: line)
    }

    public static func decodeRequest(_ line: Data) throws -> Request {
        try JSONDecoder().decode(Request.self, from: line)
    }

    private static func encoder() -> JSONEncoder {
        let e = JSONEncoder()
        // Paths and base64 contain `/`; the bridge writes it unescaped.
        e.outputFormatting = [.withoutEscapingSlashes]
        return e
    }
}
