import XCTest

/// Settings › Host resources against the demo's host (ADR 0105).
extension MacNextTests {
    @MainActor
    func testTheHostSummaryOpensHostResources() throws {
        let app = launch()
        let summary = app.buttons["host.summary"].firstMatch
        XCTAssertTrue(summary.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(summary, labelContaining: "warning"), "the demo's memory is past the host's limit")
        summary.click()

        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["host.task.t1"].waitForExistence(timeout: 10), "fix-login's agent is listed")
        XCTAssertTrue(waitFor(any["host.tile.memory"], labelContaining: "warning"))
        XCTAssertTrue(waitFor(any["host.group.simulator"], labelContaining: "Booted by Xcode or a test"))

        app.buttons["host.scan"].click()
        XCTAssertTrue(any["host.disk./demo/app-fix-login"].waitForExistence(timeout: 10))
        XCTAssertTrue(any["host.leftover.claude-worktree"].waitForExistence(timeout: 5))
    }
}
