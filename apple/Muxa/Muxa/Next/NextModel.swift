import Foundation
import MuxaSession
import MuxaTransport
import MuxaV2
import Observation

/// The app on protocol v2 (ADR 0066, 0067): one connection to a host's muxa
/// server, resumed after a network change or a return from the background.
/// It keeps the Inbox, the terminals and the Routines current from events.
@MainActor
@Observable
final class NextModel {
    enum Link: Equatable {
        case offline
        case connecting
        case connected
        /// The connection is gone; it is resumed on its own.
        case reconnecting(String)
        case failed(String)
    }

    let hosts: HostStore
    private(set) var link: Link = .offline
    private(set) var hostID: UUID?
    private(set) var demo = false
    /// A host key the user has not trusted yet; `trust()` pins it.
    var untrustedKey: HostKey?
    /// Logging in with the password to add this device's key.
    private(set) var addingKey = false
    /// The password of a key setup that waits for the user to trust the host key.
    private var setUpAfterTrust: String?
    /// The user pressed Disconnect: coming back to the foreground does not connect.
    private var stayDisconnected = false
    private(set) var inbox = Inbox.empty
    private(set) var terminals: [Terminal] = []
    private(set) var routines: [Routine] = []
    private(set) var routineProblems: [RoutineProblem] = []
    /// Why the last request failed, shown until the next one.
    var requestError: String?

    private var client: V2Client?
    private var resume = V2Client.State()
    private var events: Task<Void, Never>?
    private var retry: Task<Void, Never>?
    /// Where each attached channel's output goes (Focus).
    private var sinks: [UInt32: @MainActor (V2Event) -> Void] = [:]

    static let clientName = "muxa-ios \(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "?")"

    init(hosts: HostStore) {
        self.hosts = hosts
    }

    /// Agents waiting on the user: a short-lived state, never an Inbox item.
    var needsYou: [Terminal] {
        terminals.filter { $0.live && $0.isTaskAgent && ["blocked", "waiting"].contains($0.status?.state ?? "") }
    }

    var isConnected: Bool { link == .connected }

    // MARK: Connecting

    func connect(to id: UUID) async {
        guard let host = hosts.host(id) else { return }
        stayDisconnected = false
        if hostID != id { forget() }
        hostID = id
        demo = false
        hosts.markUsed(id)
        await open {
            try await SSHExec.connect(
                to: host.target, key: try ClientKeyStore.loadOrCreate(), command: host.target.command + " connect")
        }
    }

    /// Try the Demo: the in-app server, no host.
    func startDemo() async {
        stayDisconnected = false
        forget()
        hostID = nil
        demo = true
        await open { DemoChannel() }
    }

    var currentHost: MuxaSession.Host? { hosts.host(hostID) }

    /// Pins the presented host key, then goes on with what it stopped.
    func trust() async {
        guard let key = untrustedKey, let id = hostID, var host = hosts.host(id) else { return }
        host.target.hostKey = key
        hosts.upsert(host)
        untrustedKey = nil
        if let password = setUpAfterTrust {
            setUpAfterTrust = nil
            await setUpKey(on: id, password: password)
        } else {
            await connect(to: id)
        }
    }

    func cancelTrust() {
        untrustedKey = nil
        setUpAfterTrust = nil
        link = .offline
    }

    /// Logs in once with the password to add this device's key, restricted to
    /// the bridge, then connects. The password is not kept.
    func setUpKey(on id: UUID, password: String) async {
        guard let host = hosts.host(id) else { return }
        hostID = id
        addingKey = true
        defer { addingKey = false }
        do {
            let key = try ClientKeyStore.loadOrCreate()
            let line = try KeySetup.authorizedLine(for: key, command: host.target.command, comment: Self.keyComment)
            _ = try await KeySetup.install(line, on: host.target, password: password)
        } catch let TransportError.hostKeyMismatch(presented) {
            setUpAfterTrust = password
            untrustedKey = presented
            return
        } catch TransportError.notAuthorized {
            link = .failed("The host refused the password. Check the user and password.")
            return
        } catch {
            link = .failed(Self.describe(error))
            return
        }
        await connect(to: id)
    }

    /// The `authorized_keys` line for this device, to add by hand.
    func authorizedKeyLine(command: String) -> String? {
        guard let key = try? ClientKeyStore.loadOrCreate() else { return nil }
        return try? KeySetup.authorizedLine(for: key, command: command, comment: Self.keyComment)
    }

    private static let keyComment = "muxa-ios"

    func disconnect() async {
        stayDisconnected = true
        retry?.cancel()
        await drop()
        link = .offline
    }

    /// Leaves for the background: the server keeps the session for a resume.
    func suspend() async {
        retry?.cancel()
        await drop()
        if link == .connected { link = .offline }
    }

    /// Back in the foreground, or the network came back.
    func resumeIfNeeded() async {
        guard client == nil, link != .connecting, !stayDisconnected else { return }
        if demo { return await startDemo() }
        if let hostID { await connect(to: hostID) }
    }

    private func open(_ channel: @escaping () async throws -> any ByteChannel) async {
        retry?.cancel()
        await drop()
        link = .connecting
        do {
            let c = try await V2Client.open(try await channel(), name: Self.clientName, state: resume)
            client = c
            listen(c)
            try await c.watch()
            link = .connected
            await reload()
        } catch let TransportError.hostKeyMismatch(presented) {
            untrustedKey = presented
            link = .failed("Check the host key before you trust it.")
        } catch {
            link = .failed(Self.describe(error))
        }
    }

    private func listen(_ c: V2Client) {
        events = Task { [weak self] in
            for await e in c.events {
                guard let self else { return }
                await self.handle(e, from: c)
            }
        }
    }

    private func handle(_ e: V2Event, from c: V2Client) async {
        switch e {
        case .inbox(let i): inbox = i
        case .changed(let t):
            if let at = terminals.firstIndex(where: { $0.id == t.id }) { terminals[at] = t } else { terminals.append(t) }
        case .removed(let id): terminals.removeAll { $0.id == id }
        case .resync: terminals = (try? await c.terminals()) ?? terminals
        case .output(let ch, _), .paint(let ch), .closed(let ch, _): sinks[ch]?(e)
        case .lost(let why):
            resume = await c.resumeState
            client = nil
            link = .reconnecting(why)
            reconnectSoon()
        }
    }

    /// Tries again with growing waits while the connection is down.
    private func reconnectSoon() {
        retry = Task { [weak self] in
            var wait: Duration = .seconds(1)
            while !Task.isCancelled {
                try? await Task.sleep(for: wait)
                guard let self, !Task.isCancelled, self.client == nil else { return }
                await self.resumeIfNeeded()
                if self.isConnected { return }
                wait = min(wait * 2, .seconds(30))
            }
        }
    }

    private func reload() async {
        guard let c = client else { return }
        do {
            inbox = try await c.inbox()
            terminals = try await c.terminals()
            (routines, routineProblems) = try await c.routines()
        } catch {
            requestError = Self.describe(error)
        }
    }

    private func drop() async {
        events?.cancel()
        if let c = client {
            resume = await c.resumeState
            await c.close()
        }
        client = nil
    }

    /// A new host or the demo: nothing of the old session carries over.
    private func forget() {
        resume = V2Client.State()
        sinks = [:]
        inbox = .empty
        terminals = []
        routines = []
        routineProblems = []
    }

    // MARK: Requests

    /// Runs an Inbox request; the reply replaces the Inbox.
    func inbox(_ r: V2Request) async {
        await run { self.inbox = try await $0.inbox(r) }
    }

    func parseCapture(_ text: String) async -> Capture? {
        var parsed: Capture?
        await run { parsed = try await $0.parseCapture(text, today: Self.today()) }
        return parsed
    }

    func runRoutine(_ r: Routine) async {
        await run {
            try await $0.runRoutine(workspace: r.workspace, name: r.name)
            (self.routines, self.routineProblems) = try await $0.routines()
        }
    }

    func refreshRoutines() async {
        await run { (self.routines, self.routineProblems) = try await $0.routines() }
    }

    private func run(_ body: (V2Client) async throws -> Void) async {
        guard let c = client else {
            requestError = "Not connected."
            return
        }
        do {
            requestError = nil
            try await body(c)
        } catch {
            requestError = Self.describe(error)
        }
    }

    // MARK: Terminals (Focus)

    /// Attaches `terminal`; its events go to `sink` until `detach`.
    func attach(_ terminal: String, cols: UInt16, rows: UInt16, sink: @escaping @MainActor (V2Event) -> Void) async -> UInt32? {
        guard let c = client else { return nil }
        do {
            let ch = try await c.attach(terminal, cols: cols, rows: rows, history: 2000)
            sinks[ch] = sink
            return ch
        } catch {
            requestError = Self.describe(error)
            return nil
        }
    }

    func detach(_ channel: UInt32) async {
        sinks[channel] = nil
        try? await client?.detach(channel)
    }

    func input(_ channel: UInt32, _ bytes: [UInt8]) async {
        try? await client?.input(channel, bytes)
    }

    func resize(_ channel: UInt32, cols: UInt16, rows: UInt16) async {
        await client?.resize(channel, cols: cols, rows: rows)
    }

    // MARK: Helpers

    /// The device's local date, `YYYY-MM-DD`, as Capture reads `due`.
    static func today(_ now: Date = .now) -> String {
        let c = Calendar.current.dateComponents([.year, .month, .day], from: now)
        return String(format: "%04d-%02d-%02d", c.year ?? 0, c.month ?? 0, c.day ?? 0)
    }

    static func describe(_ error: any Error) -> String {
        switch error {
        case V2Error.refused(let code, let message) where code == "version":
            return "The host runs an older muxa: \(message). Update it with make install on the host."
        case V2Error.refused(_, let message): return message
        case V2Error.lost(let why): return "The connection was lost: \(why)"
        case TransportError.notAuthorized: return "The host refused this device's key. Add it in Settings."
        case TransportError.failed(let why): return why
        default: return error.localizedDescription
        }
    }
}
