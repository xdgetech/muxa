import Foundation
import MuxaProtocol
import MuxaSession
import MuxaTransport
import Observation

/// App state: the saved hosts, the connection, and which pane is shown.
@MainActor
@Observable
final class AppModel {
    let session = PaneSession()
    let hosts: HostStore
    /// The host the session belongs to: connected, connecting, or last tried.
    private(set) var currentHostID: UUID?
    private(set) var connecting = false
    /// A host key waiting for the user's decision on first use (or after it changed).
    var untrustedKey: HostKey?
    var connectError: String?
    var showSwitcher = false
    /// Bumped when a sheet closes, so the pane takes the keyboard back.
    private(set) var focusRequest = 0
    /// Set by a DEBUG launch argument: an in-app bridge instead of SSH.
    let demo: Bool

    /// What to do once the user trusts the presented host key. A setup password is
    /// held only while the trust sheet is up.
    private enum AfterTrust {
        case connect
        case setUp(password: String)
    }
    private var afterTrust: AfterTrust?
    /// The user pressed Disconnect: returning to the foreground does not reconnect.
    private var stayDisconnected = false

    init() {
        #if DEBUG
        demo = ProcessInfo.processInfo.arguments.contains("-demo")
        #else
        demo = false
        #endif
        if demo {
            hosts = HostStore(file: nil)
            let host = Host(name: "Demo", target: SSHTarget(host: Self.demoHost, user: "demo"))
            hosts.upsert(host)
            hosts.markUsed(host.id)
        } else {
            hosts = HostStore(file: HostStore.defaultFile)
        }
    }

    // MARK: Connection

    var isConnected: Bool { session.phase == .connected }
    var currentHost: Host? { hosts.host(currentHostID) }

    func connect(to id: UUID) async {
        guard let host = hosts.host(id) else { return }
        let sameHost = id == currentHostID
        currentHostID = id
        hosts.markUsed(id)
        stayDisconnected = false
        connectError = nil
        connecting = true
        defer { connecting = false }
        do {
            try await session.connect({ try await self.open(host) }, samePanes: sameHost)
            await openDefaultPane()
        } catch TransportError.hostKeyMismatch(let presented) {
            afterTrust = .connect
            untrustedKey = presented
        } catch {
            connectError = PaneSession.describe(error)
        }
    }

    private func open(_ host: Host) async throws -> any BridgeChannel {
        #if DEBUG
        if demo, host.target.host == Self.demoHost { return DemoBridge() }
        #endif
        return try await SSHBridge.connect(to: host.target, key: ClientKeyStore.loadOrCreate())
    }

    func trust(_ key: HostKey) async {
        untrustedKey = nil
        let next = afterTrust
        afterTrust = nil
        guard var host = currentHost else { return }
        host.target.hostKey = key
        hosts.upsert(host)
        switch next {
        case .setUp(let password): await setUpKey(on: host.id, password: password)
        case .connect, nil: await connect(to: host.id)
        }
    }

    func cancelTrust() {
        untrustedKey = nil
        afterTrust = nil
    }

    /// Logs in once with `password`, adds this device's key to the host, then connects.
    func setUpKey(on id: UUID, password: String) async {
        guard let host = hosts.host(id) else { return }
        currentHostID = id
        connectError = nil
        do {
            let key = try ClientKeyStore.loadOrCreate()
            let line = try KeySetup.authorizedLine(for: key, command: host.target.command, comment: Self.keyComment)
            _ = try await KeySetup.install(line, on: host.target, password: password)
            await connect(to: id)
        } catch TransportError.hostKeyMismatch(let presented) {
            afterTrust = .setUp(password: password)
            untrustedKey = presented
        } catch TransportError.notAuthorized {
            connectError = "The host refused the password. Check the user and password."
        } catch {
            connectError = PaneSession.describe(error)
        }
    }

    func disconnect() async {
        stayDisconnected = true
        await session.disconnect()
    }

    /// Leaving for the background: close cleanly so the bridge exits, and come back later.
    func suspend() async {
        await session.disconnect()
    }

    /// Launch or return to the foreground: reconnect to the last host and pane.
    func resume() async {
        guard !stayDisconnected, !isConnected, !connecting else { return }
        guard let id = currentHostID ?? hosts.lastUsed, hosts.host(id) != nil else { return }
        await connect(to: id)
    }

    func refocusPane() { focusRequest += 1 }

    /// The public key line for `~/.ssh/authorized_keys`, restricted to `command`.
    func authorizedKeyLine(command: String) -> String? {
        guard let key = try? ClientKeyStore.loadOrCreate() else { return nil }
        return try? KeySetup.authorizedLine(for: key, command: command, comment: Self.keyComment)
    }

    private static let keyComment = "muxa-ipad"
    private static let demoHost = "demo.invalid"

    // MARK: Panes

    var visibleSessions: [Session] {
        session.sessions.filter { $0.defaultPane != nil }
    }

    var currentSession: Session? {
        guard let pane = session.watchedPane else { return nil }
        return session.sessions.first { $0.panes.contains { $0.id == pane } }
    }

    var currentPane: Pane? {
        guard let id = session.watchedPane else { return nil }
        return currentSession?.panes.first { $0.id == id }
    }

    var counterpart: Pane? {
        guard let id = session.watchedPane else { return nil }
        return currentSession?.counterpart(of: id)
    }

    func open(_ s: Session) async {
        guard let pane = s.defaultPane else { return }
        await watch(pane.id)
    }

    func toggleAgentTerminal() async {
        guard let other = counterpart else { return }
        await watch(other.id)
    }

    /// Moves `offset` sessions through the list, wrapping around.
    func step(_ offset: Int) async {
        let list = visibleSessions
        guard !list.isEmpty else { return }
        let index = list.firstIndex { $0.id == currentSession?.id } ?? 0
        await open(list[(index + offset + list.count) % list.count])
    }

    /// Cmd+1…9: the nth session, 1-based.
    func openSession(number: Int) async {
        let list = visibleSessions
        guard number >= 1, number <= list.count else { return }
        await open(list[number - 1])
    }

    private func openDefaultPane() async {
        guard session.watchedPane == nil, let first = visibleSessions.first else { return }
        await open(first)
    }

    private func watch(_ pane: String) async {
        do {
            try await session.watch(pane: pane)
        } catch {
            connectError = PaneSession.describe(error)
        }
    }
}
