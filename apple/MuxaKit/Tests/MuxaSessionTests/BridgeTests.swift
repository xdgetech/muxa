#if os(macOS)
import Foundation
import MuxaTransport
import Testing

@testable import MuxaSession

/// PaneSession against the real bridge on a throwaway tmux server (`-L muxa-test-swift-*`).
/// Opt-in: `make test-apple-tmux` sets MUXA_TEST_BRIDGE to a built muxa-bridge.
@MainActor
@Suite(.enabled(if: ProcessInfo.processInfo.environment["MUXA_TEST_BRIDGE"] != nil))
struct BridgeTests {
    let bridge = ProcessInfo.processInfo.environment["MUXA_TEST_BRIDGE"] ?? ""
    let socket = "muxa-test-swift-\(getpid())-\(Int.random(in: 0..<1_000_000))"

    func tmux(_ args: String...) throws -> String {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        p.arguments = ["tmux", "-L", socket, "-f", "/dev/null"] + args
        var env = ProcessInfo.processInfo.environment
        env["TMUX"] = nil
        env["PATH"] = "/opt/homebrew/bin:/usr/local/bin:" + (env["PATH"] ?? "/usr/bin:/bin")
        p.environment = env
        let out = Pipe()
        p.standardOutput = out
        try p.run()
        p.waitUntilExit()
        return String(decoding: out.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    @Test func watchTypeAndLeaveTheLayoutAlone() async throws {
        _ = try tmux("new-session", "-d", "-s", "t", "-x", "40", "-y", "6", "printf 'ready\\n'; exec cat")
        let path = try tmux("display-message", "-p", "#{socket_path}")
        defer {
            _ = try? tmux("kill-server")
            // tmux leaves its socket file behind.
            try? FileManager.default.removeItem(atPath: path)
        }
        let before = try tmux("display-message", "-p", "-t", "t", "#{window_width}x#{window_height}")

        let session = PaneSession()
        let screen = HeadlessScreen()
        session.renderer = screen
        let command = "'\(bridge)' --socket '\(socket)'"
        try await session.connect { try ProcessBridge(command: command) }
        let pane = try #require(session.sessions.first?.panes.first)

        try await session.watch(pane: pane.id)
        try await waitUntil { screen.rows.first == "ready" }
        session.send(text: "hi")
        session.send(keys: ["Enter"])
        // The tty echoes "hi", then cat prints it again.
        try await waitUntil { screen.rows == ["ready", "hi", "hi"] }
        #expect(screen.terminal.cols == Int(pane.cols))

        await session.disconnect()
        #expect(try tmux("display-message", "-p", "-t", "t", "#{window_width}x#{window_height}") == before)
    }
}
#endif
