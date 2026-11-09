import Foundation
import MuxaProtocol
import Observation

/// The assistant Results this device has shown, so the Inbox marks a new one as ready
/// (ADR 0046 §10). Kept per host as ids and versions in one file; never the Result's text.
@MainActor
@Observable
public final class SeenResults {
    /// Keys of `host request version`, oldest first.
    public private(set) var keys: [String] = []

    private let file: URL?
    /// Older keys go first past this many.
    static let limit = 500

    /// Loads `file`. A nil `file` keeps them in memory, as the demo does. An unreadable
    /// file only shows a seen Result as ready once more.
    public init(file: URL?) {
        self.file = file
        guard let file, let data = try? Data(contentsOf: file) else { return }
        keys = (try? JSONDecoder().decode([String].self, from: data)) ?? []
    }

    /// The default location: Application Support/seen-results.json.
    public static var defaultFile: URL {
        URL.applicationSupportDirectory.appending(path: "seen-results.json")
    }

    /// The request's current Result is not shown on this device yet.
    public func isNew(_ request: AssistantRequest, on host: UUID?) -> Bool {
        guard let result = request.currentResult else { return false }
        return !keys.contains(Self.key(host, request.id, result.version))
    }

    public func markSeen(_ request: AssistantRequest, on host: UUID?) {
        guard let result = request.currentResult else { return }
        let key = Self.key(host, request.id, result.version)
        guard !keys.contains(key) else { return }
        keys = Array((keys + [key]).suffix(Self.limit))
        save()
    }

    private static func key(_ host: UUID?, _ request: String, _ version: UInt64) -> String {
        "\(host?.uuidString ?? "-") \(request) \(version)"
    }

    private func save() {
        guard let file else { return }
        do {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            try JSONEncoder().encode(keys).write(to: file, options: [.atomic])
        } catch {
            // Losing it shows a seen Result as ready again; nothing else depends on it.
            return
        }
    }
}
