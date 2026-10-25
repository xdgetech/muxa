import XCTest

/// Drives the app against its in-app demo bridge (`-demo`): no host needed.
final class DemoTests: XCTestCase {
    @MainActor
    func testTypingReachesThePaneAndShortcutsSwitch() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()

        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))

        pane.typeText("hello")
        XCTAssertTrue(waitFor(pane, containing: "$ hello"))
        pane.typeKey("c", modifierFlags: .control)
        XCTAssertTrue(waitFor(pane, containing: "<C-c>"))

        // Cmd+J: the terminal side of the same session.
        pane.typeKey("j", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %2 100x30"))
        // Cmd+2: the second session, a bigger pane.
        pane.typeKey("2", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %3 132x43"))
        save("portrait-132x43")
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(waitFor(pane, containing: "pane %3 132x43"))
        save("landscape-132x43")
        XCUIDevice.shared.orientation = .portrait
    }

    /// Set MUXA_SCREENSHOT_DIR (TEST_RUNNER_MUXA_SCREENSHOT_DIR for xcodebuild) to keep screenshots.
    private func save(_ name: String) {
        guard let dir = ProcessInfo.processInfo.environment["MUXA_SCREENSHOT_DIR"] else { return }
        sleep(1)
        let png = XCUIScreen.main.screenshot().pngRepresentation
        try? png.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
    }

    private func waitFor(_ element: XCUIElement, containing text: String) -> Bool {
        let predicate = NSPredicate(format: "value CONTAINS %@", text)
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [exp], timeout: 5) == .completed
    }
}
