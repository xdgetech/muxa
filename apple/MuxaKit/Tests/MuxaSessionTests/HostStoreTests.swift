import Foundation
import MuxaTransport
import Testing

@testable import MuxaSession

@MainActor
struct HostStoreTests {
    private let dir = FileManager.default.temporaryDirectory.appending(path: "muxa-hosts-\(UUID().uuidString)")
    private var file: URL { dir.appending(path: "hosts.json") }

    private func freshDefaults() -> UserDefaults {
        let name = "muxa-test-\(UUID().uuidString)"
        let d = UserDefaults(suiteName: name)!
        d.removePersistentDomain(forName: name)
        return d
    }

    @Test func savedHostsSurviveAReload() throws {
        defer { try? FileManager.default.removeItem(at: dir) }
        let store = HostStore(file: file, defaults: freshDefaults())
        let a = Host(name: "mini", target: SSHTarget(host: "mini.example", user: "me"))
        let b = Host(name: "", target: SSHTarget(host: "box.example", port: 2222, user: "dev"))
        store.upsert(a)
        store.upsert(b)
        store.markUsed(b.id)
        store.move(fromOffsets: [1], toOffset: 0)
        var edited = a
        edited.target.hostKey = try HostKey(openSSH: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICmVLvUbABi5WZoMHwkY/jy0DxNDGNMw2TRDgtxMPf9x")
        store.upsert(edited)

        let again = HostStore(file: file, defaults: freshDefaults())
        #expect(again.hosts == [b, edited])
        #expect(again.lastUsed == b.id)
        #expect(again.hosts[0].title == "box.example")
        #expect(again.storageError == nil)

        again.delete(b.id)
        #expect(HostStore(file: file, defaults: freshDefaults()).hosts == [edited])
        #expect(again.lastUsed == nil)
    }

    @Test func movesTheOldSingleTargetOverOnce() throws {
        defer { try? FileManager.default.removeItem(at: dir) }
        let defaults = freshDefaults()
        let old = SSHTarget(host: "mini.example", user: "me")
        defaults.set(try JSONEncoder().encode(old), forKey: "target")

        let store = HostStore(file: file, defaults: defaults)
        #expect(store.hosts.map(\.target) == [old])
        #expect(store.lastUsed == store.hosts.first?.id)
        #expect(defaults.data(forKey: "target") == nil)
        #expect(HostStore(file: file, defaults: defaults).hosts.map(\.target) == [old])
    }

    @Test func aDamagedFileIsReportedNotOverwritten() throws {
        defer { try? FileManager.default.removeItem(at: dir) }
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        try Data("not json".utf8).write(to: file)
        let store = HostStore(file: file, defaults: freshDefaults())
        #expect(store.hosts.isEmpty)
        #expect(store.storageError != nil)
        #expect(try Data(contentsOf: file) == Data("not json".utf8))
    }
}
