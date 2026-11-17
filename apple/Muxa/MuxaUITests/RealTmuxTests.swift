import XCTest

/// The app against the real bridge on a throwaway tmux server: `make test-app-tmux`
/// starts both. The session "shell" printed 500 rows; "agent" printed 500 and clears
/// tmux's history whenever its window is resized. Skipped in the demo runs.
final class RealTmuxTests: XCTestCase {
    @MainActor
    func testAFingerScrollsBackToTheShellsFirstRow() throws {
        let app = try launch()
        scrollBack(app, session: "shell", newest: "row 500", oldest: "row 001")
    }

    /// The device's size redraws the agent, which clears tmux's history.
    @MainActor
    func testAFingerScrollsBackToTheAgentsFirstRowAfterItsResize() throws {
        let app = try launch()
        scrollBack(app, session: "agent", newest: "agent redrawn", oldest: "agent 001")
    }

    @MainActor
    private func scrollBack(_ app: XCUIApplication, session: String, newest: String, oldest: String) {
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(pane.waitForExistence(timeout: 20), "no pane")
        open(app, session: session, showing: newest)
        let first = app.descendants(matching: .any).matching(NSPredicate(format: "value CONTAINS %@", oldest)).firstMatch
        for _ in 0..<40 where !first.exists {
            let target = app.descendants(matching: .any).matching(
                NSPredicate(format: "identifier IN {'pane', 'pane-history', 'history'}")
            ).allElementsBoundByIndex.last { $0.isHittable } ?? pane
            target.swipeDown(velocity: .fast)
        }
        save("real-tmux-\(session)")
        XCTAssertTrue(first.exists, "a finger did not scroll back to \(oldest)")
    }

    /// Cmd+1 or Cmd+2 in the iPad sidebar; More, Sessions…, its row on a phone.
    /// The pane's own text tells which session shows: the sidebar lists both names.
    @MainActor
    private func open(_ app: XCUIApplication, session: String, showing newest: String) {
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        let shows = { () -> Bool in
            let exp = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value CONTAINS %@", newest), object: pane)
            return XCTWaiter.wait(for: [exp], timeout: 8) == .completed
        }
        // The app opens a session itself.
        if shows() { return }
        if UIDevice.current.userInterfaceIdiom == .pad {
            for key in ["1", "2"] {
                pane.typeKey(key, modifierFlags: .command)
                if shows() { return }
            }
        } else {
            tapWhenReady(app.buttons["More"])
            tapWhenReady(app.buttons["Sessions…"])
            tapWhenReady(app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@", session)).firstMatch)
            if shows() { return }
        }
        XCTFail("\(session) did not show")
    }

    @MainActor
    private func launch() throws -> XCUIApplication {
        let port = ProcessInfo.processInfo.environment["MUXA_BRIDGE_PORT"]
        try XCTSkipIf(port == nil, "needs make test-app-tmux")
        let app = XCUIApplication()
        app.launchArguments = ["-bridge-port", port!, "-mode", "sessions"]
        app.launch()
        return app
    }
}
