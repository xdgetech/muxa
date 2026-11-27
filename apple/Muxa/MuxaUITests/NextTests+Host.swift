import XCTest

/// Settings › Host resources against the demo's host (ADR 0105).
extension NextTests {
    @MainActor
    func testTheHostSummaryOpensHostResources() throws {
        let app = launch()
        let summary = app.buttons["host.summary"]
        XCTAssertTrue(summary.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(summary, labelContaining: "warning"), "the demo's memory is past the host's limit")
        summary.tap()

        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["host.task.t1"].waitForExistence(timeout: 10), "fix-login's agent is listed")
        XCTAssertTrue(waitFor(any["host.task.t7"], labelContaining: "idle 2 d"))
        XCTAssertTrue(waitFor(any["host.tile.memory"], labelContaining: "warning"))
        let simulator = any["host.group.simulator"]
        scrollTo(simulator, in: app)
        XCTAssertTrue(waitFor(simulator, labelContaining: "Booted by Xcode or a test"))

        let scan = app.buttons["host.scan"]
        scrollTo(scan, in: app)
        scan.tap()
        let place = any["host.disk./demo/app-fix-login"]
        scrollTo(place, in: app)
        XCTAssertTrue(waitFor(place, labelContaining: "build"))
        let leftover = any["host.leftover.claude-worktree"]
        scrollTo(leftover, in: app)
        XCTAssertTrue(waitFor(leftover, labelContaining: "cleanable"))
    }
}
