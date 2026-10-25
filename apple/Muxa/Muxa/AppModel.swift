import Foundation
import MuxaProtocol
import MuxaSession
import MuxaTransport
import Observation

/// App state: the saved host, the connection, and which pane is shown.
@MainActor
@Observable
final class AppModel {
    let session = PaneSession()
    var target: SSHTarget? {
        didSet { Self.save(target) }
    }
    /// A host key waiting for the user's decision on first use (or after it changed).
    var untrustedKey: HostKey?
    var connectError: String?
    var setupMessage: String?
    /// Kept only while the trust sheet is up during key setup, then cleared.
    private var pendingSetupPassword: String?
    var showSwitcher = false
    /// Set by a DEBUG launch argument: an in-app bridge instead of SSH.
    var demo = false

    init() {
        target = Self.load()
        #if DEBUG
        demo = ProcessInfo.processInfo.arguments.contains("-demo")
        #endif
    }

    // MARK: Connection

    var isConnected: Bool { session.phase == .connected }

    func connect() async {
        connectError = nil
        if demo {
            #if DEBUG
            try? await session.connect { DemoBridge() }
            await openDefaultPane()
            #endif
            return
        }
        guard let target else { return }
        do {
            let key = try ClientKeyStore.loadOrCreate()
            try await session.connect { try await SSHBridge.connect(to: target, key: key) }
            await openDefaultPane()
        } catch TransportError.hostKeyMismatch(let presented) {
            untrustedKey = presented
        } catch {
            connectError = PaneSession.describe(error)
        }
    }

    func trust(_ key: HostKey) async {
        untrustedKey = nil
        target?.hostKey = key
        if let password = pendingSetupPassword {
            await setUpKey(password: password)
        } else {
            await connect()
        }
    }

    func cancelTrust() {
        untrustedKey = nil
        pendingSetupPassword = nil
    }

    /// Logs in once with `password`, adds this device's key to the host, then connects.
    func setUpKey(password: String) async {
        pendingSetupPassword = nil
        connectError = nil
        setupMessage = nil
        guard let target else { return }
        do {
            let key = try ClientKeyStore.loadOrCreate()
            let line = try KeySetup.authorizedLine(for: key, command: target.command, comment: Self.keyComment)
            _ = try await KeySetup.install(line, on: target, password: password)
            await connect()
        } catch TransportError.hostKeyMismatch(let presented) {
            pendingSetupPassword = password
            untrustedKey = presented
        } catch TransportError.notAuthorized {
            setupMessage = "The host refused the password. Check the user and password."
        } catch {
            setupMessage = PaneSession.describe(error)
        }
    }

    func disconnect() async {
        await session.disconnect()
    }

    /// The scene came back: reconnect and show the same pane again.
    func resume() async {
        guard target != nil || demo else { return }
        if case .connected = session.phase { return }
        await connect()
    }

    /// The public key line for `~/.ssh/authorized_keys`, restricted to the bridge.
    func authorizedKeyLine() -> String? {
        guard let key = try? ClientKeyStore.loadOrCreate() else { return nil }
        return try? KeySetup.authorizedLine(
            for: key, command: target?.command ?? SSHTarget.defaultCommand, comment: Self.keyComment)
    }

    private static let keyComment = "muxa-ipad"

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

    // MARK: Storage

    private static let key = "target"

    private static func load() -> SSHTarget? {
        guard let data = UserDefaults.standard.data(forKey: key) else { return nil }
        return try? JSONDecoder().decode(SSHTarget.self, from: data)
    }

    private static func save(_ target: SSHTarget?) {
        guard let target, let data = try? JSONEncoder().encode(target) else {
            UserDefaults.standard.removeObject(forKey: key)
            return
        }
        UserDefaults.standard.set(data, forKey: key)
    }
}
