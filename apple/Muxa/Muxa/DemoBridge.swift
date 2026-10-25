#if DEBUG
import Foundation
import MuxaProtocol
import MuxaTransport

/// An in-app bridge for the simulator (launch argument `-demo`): two sessions,
/// fixed-size panes, and typed text echoed back. No SSH, no tmux.
final class DemoBridge: BridgeChannel, @unchecked Sendable {
    let lines: AsyncThrowingStream<Data, any Error>
    private let out: AsyncThrowingStream<Data, any Error>.Continuation
    private let lock = NSLock()
    private var watch: UInt64 = 0

    static let sessions = [
        Session(id: "$1", name: "feature-auth", repo: "app", branch: "feat/auth", panes: [
            Pane(id: "%1", window: "@1", index: 0, role: .agent, cols: 100, rows: 30, command: "claude",
                 path: "/work", dead: false, shellForeground: false,
                 agent: Agent(state: .waiting, since: UInt64(Date().timeIntervalSince1970) - 90)),
            Pane(id: "%2", window: "@1", index: 1, role: .shell, cols: 100, rows: 30, command: "zsh",
                 path: "/work", dead: false, shellForeground: true),
        ]),
        Session(id: "$2", name: "scratch", panes: [
            Pane(id: "%3", window: "@2", index: 0, role: .unknown, cols: 132, rows: 43, command: "vim",
                 path: "/tmp", dead: false, shellForeground: false),
        ]),
    ]

    init() {
        (lines, out) = AsyncThrowingStream<Data, any Error>.makeStream()
        emit(.hello(protocol: protocolVersion, bridge: "demo", tmux: "demo", capabilities: []))
    }

    func send(_ line: Data) async throws {
        let req = try Wire.decodeRequest(line)
        switch req {
        case .list(let id):
            emit(.result(id: id, error: nil, sessions: Self.sessions))
        case .watch(let id, let pane):
            lock.withLock { watch = id }
            emit(.result(id: id, error: nil, sessions: nil))
            guard let p = Self.sessions.flatMap(\.panes).first(where: { $0.id == pane }) else { return }
            emit(.screen(watch: id, pane: pane, cols: p.cols, rows: p.rows, data: Self.screen(for: p)))
        case .text(let id, let pane, let text):
            ok(id)
            echo(pane, Data(text.utf8))
        case .paste(let id, let pane, let text):
            ok(id)
            echo(pane, Data(text.replacingOccurrences(of: "\n", with: "\r\n").utf8))
        case .keys(let id, let pane, let keys):
            ok(id)
            for key in keys {
                switch key {
                case "Enter": echo(pane, Data("\r\n$ ".utf8))
                case "BSpace": echo(pane, Data("\u{8} \u{8}".utf8))
                default: echo(pane, Data("\u{1b}[7m<\(key)>\u{1b}[0m".utf8))
                }
            }
        case .bye:
            out.finish()
        default:
            ok(req.id)
        }
    }

    func close() async { out.finish() }

    private func ok(_ id: UInt64) { emit(.result(id: id, error: nil, sessions: nil)) }

    private func echo(_ pane: String, _ data: Data) {
        let w = lock.withLock { watch }
        emit(.output(watch: w, pane: pane, data: data))
    }

    private func emit(_ m: Message) {
        if let line = try? Wire.encode(m) { out.yield(line) }
    }

    /// A frame that shows whether the grid is exact: corners, a ruler and the last row.
    static func screen(for p: Pane) -> Data {
        let cols = Int(p.cols), rows = Int(p.rows)
        var s = "\u{1b}c"
        let ruler = (0..<cols).map { String($0 % 10) }.joined()
        s += "\u{1b}[1;1H\u{1b}[36m" + ruler + "\u{1b}[0m"
        s += "\u{1b}[2;1H\u{1b}[1m\(p.command) — pane \(p.id) \(cols)x\(rows)\u{1b}[0m"
        s += "\u{1b}[4;1H日本語 ✓ é  \u{1b}[31mred\u{1b}[32m green\u{1b}[34m blue\u{1b}[0m"
        s += "\u{1b}[\(rows);1H\u{1b}[7m" + String(repeating: " ", count: cols - 1) + "\u{1b}[0m"
        s += "\u{1b}[\(rows);\(cols)HX"
        s += "\u{1b}[6;1H$ "
        return Data(s.utf8)
    }
}
#endif
