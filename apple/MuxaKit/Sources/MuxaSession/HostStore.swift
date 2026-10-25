import Foundation
import MuxaTransport
import Observation

/// A saved host: a name the user picked and how to reach the bridge there.
public struct Host: Codable, Identifiable, Hashable, Sendable {
    public var id: UUID
    public var name: String
    public var target: SSHTarget

    public init(id: UUID = UUID(), name: String, target: SSHTarget) {
        self.id = id
        self.name = name
        self.target = target
    }

    /// The name, or the address when the user left it empty.
    public var title: String { name.isEmpty ? target.host : name }
}

/// The saved hosts and the last one used, as JSON in one file. Passwords are never
/// stored; the pinned host key is public.
@MainActor
@Observable
public final class HostStore {
    public private(set) var hosts: [Host] = []
    public private(set) var lastUsed: UUID?
    /// Why the last load or save failed, for the UI. Nil when it worked.
    public private(set) var storageError: String?

    private let file: URL?

    private struct Contents: Codable {
        var hosts: [Host]
        var lastUsed: UUID?
    }

    /// Loads `file`. The first time, it moves over the single target that older
    /// builds kept in `defaults` under `legacyKey`. A nil `file` keeps hosts in memory.
    public init(file: URL?, defaults: UserDefaults = .standard, legacyKey: String = "target") {
        self.file = file
        guard let file else { return }
        if let data = try? Data(contentsOf: file) {
            do {
                let contents = try JSONDecoder().decode(Contents.self, from: data)
                hosts = contents.hosts
                lastUsed = contents.lastUsed
            } catch {
                storageError = "Saved hosts could not be read: \(error.localizedDescription)"
            }
            return
        }
        if let data = defaults.data(forKey: legacyKey),
            let target = try? JSONDecoder().decode(SSHTarget.self, from: data)
        {
            let host = Host(name: target.host, target: target)
            hosts = [host]
            lastUsed = host.id
            if save() { defaults.removeObject(forKey: legacyKey) }
        }
    }

    /// The default location: Application Support/hosts.json.
    public static var defaultFile: URL {
        URL.applicationSupportDirectory.appending(path: "hosts.json")
    }

    public func host(_ id: UUID?) -> Host? {
        hosts.first { $0.id == id }
    }

    /// Adds `host`, or replaces the saved one with the same id.
    public func upsert(_ host: Host) {
        if let i = hosts.firstIndex(where: { $0.id == host.id }) {
            hosts[i] = host
        } else {
            hosts.append(host)
        }
        save()
    }

    public func delete(_ id: UUID) {
        hosts.removeAll { $0.id == id }
        if lastUsed == id { lastUsed = nil }
        save()
    }

    public func move(fromOffsets source: IndexSet, toOffset destination: Int) {
        hosts.move(fromOffsets: source, toOffset: destination)
        save()
    }

    public func markUsed(_ id: UUID) {
        guard lastUsed != id else { return }
        lastUsed = id
        save()
    }

    @discardableResult
    private func save() -> Bool {
        guard let file else { return true }
        do {
            try FileManager.default.createDirectory(
                at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
            let data = try JSONEncoder().encode(Contents(hosts: hosts, lastUsed: lastUsed))
            try data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            storageError = nil
            return true
        } catch {
            storageError = "Hosts could not be saved: \(error.localizedDescription)"
            return false
        }
    }
}
