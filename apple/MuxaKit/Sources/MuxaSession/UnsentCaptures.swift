import Foundation
import Observation

/// A capture saved while the connection was down. It keeps its `clientId`, so the host
/// stores it once however often it is sent.
public struct UnsentCapture: Codable, Equatable, Identifiable, Sendable {
    public var clientId: String
    /// The host it was meant for; it is sent only there.
    public var host: UUID
    public var text: String
    public var workspace: String?
    public var createdAt: Date
    /// Why the host refused it the last time, if it did.
    public var problem: String?

    public var id: String { clientId }

    public init(
        clientId: String, host: UUID, text: String, workspace: String? = nil, createdAt: Date = Date(),
        problem: String? = nil
    ) {
        self.clientId = clientId
        self.host = host
        self.text = text
        self.workspace = workspace
        self.createdAt = createdAt
        self.problem = problem
    }
}

/// Captures not sent yet, oldest first, as JSON in one file on this device. They stay
/// until the host has them or the user deletes them. The text is never logged.
@MainActor
@Observable
public final class UnsentCaptures {
    public private(set) var all: [UnsentCapture] = []
    /// Why the last load or save failed, for the UI. Nil when it worked.
    public private(set) var storageError: String?

    private let file: URL?

    /// Loads `file`. A nil `file` keeps them in memory, as the demo does.
    public init(file: URL?) {
        self.file = file
        guard let file, let data = try? Data(contentsOf: file) else { return }
        do {
            all = try JSONDecoder().decode([UnsentCapture].self, from: data)
        } catch {
            storageError = "Unsent captures could not be read: \(error.localizedDescription)"
        }
    }

    /// The default location: Application Support/unsent-captures.json.
    public static var defaultFile: URL {
        URL.applicationSupportDirectory.appending(path: "unsent-captures.json")
    }

    /// The ones for `host`, oldest first: the order they are sent in.
    public func on(_ host: UUID?) -> [UnsentCapture] {
        all.filter { $0.host == host }
    }

    /// Keeps `capture`; one with the same `clientId` is replaced.
    public func add(_ capture: UnsentCapture) {
        if let i = all.firstIndex(where: { $0.clientId == capture.clientId }) {
            all[i] = capture
        } else {
            all.append(capture)
        }
        save()
    }

    public func remove(_ clientId: String) {
        guard all.contains(where: { $0.clientId == clientId }) else { return }
        all.removeAll { $0.clientId == clientId }
        save()
    }

    public func setProblem(_ clientId: String, _ problem: String?) {
        guard let i = all.firstIndex(where: { $0.clientId == clientId }), all[i].problem != problem else { return }
        all[i].problem = problem
        save()
    }

    private func save() {
        guard let file else { return }
        do {
            try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            let data = try JSONEncoder().encode(all)
            try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            storageError = nil
        } catch {
            storageError = "Unsent captures could not be saved: \(error.localizedDescription)"
        }
    }
}
