import XCTest

/// The phone layout against the demo bridge. Skipped on an iPad.
final class PhoneTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUpWithError() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "phone layout only")
    }

    /// A 100-column pane is wider than a phone at a readable font. The grid stays
    /// 100 columns, and the view scrolls sideways to follow the cursor.
    @MainActor
    func testAWidePaneIsReadableAndFollowsTheCursorSideways() throws {
        let app = launchOnAgentTerminal()
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        let rows = (pane.value as? String ?? "").components(separatedBy: "\n")
        XCTAssertTrue(rows.contains { $0.count == 100 && $0.hasSuffix("X") }, "no 100-column last row")

        let window = app.windows.firstMatch.frame
        sleep(1)
        XCTAssertGreaterThan(pane.frame.width, window.width, "the font went below the readable floor")
        XCTAssertEqual(pane.frame.minX, window.minX, accuracy: 1, "a fresh screen starts at its left edge")
        save("phone-readable")

        // The pane is wider than the screen, so XCUITest finds no hit point on it.
        pane.coordinate(withNormalizedOffset: CGVector(dx: 0.2, dy: 0.5)).tap()
        // A fresh simulator's keyboard first shows a tip about sliding to type.
        let tip = app.buttons["Continue"]
        if tip.waitForExistence(timeout: 2) { tip.tap() }
        XCTAssertTrue(waitForFocus(pane))
        let long = String(repeating: "x", count: 90)
        pane.typeText(long)
        XCTAssertTrue(waitFor(pane, containing: "$ \(long)"))
        sleep(1)
        XCTAssertLessThan(pane.frame.minX, window.minX - 20, "the view did not follow the cursor to the right")
        save("phone-readable-cursor-right")
    }

    /// A pinch in shows the whole pane; a pinch out goes back to a readable font.
    @MainActor
    func testPinchSwitchesBetweenTheWholePaneAndReadable() throws {
        let app = launchOnAgentTerminal()
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        let window = app.windows.firstMatch.frame

        pane.pinch(withScale: 0.4, velocity: -1)
        sleep(1)
        XCTAssertLessThanOrEqual(pane.frame.width, window.width + 1)
        save("phone-whole")

        pane.pinch(withScale: 2.5, velocity: 1)
        sleep(1)
        XCTAssertGreaterThan(pane.frame.width, window.width)
    }

    /// The demo opens the agent pane in Chat; this shows its terminal.
    @MainActor
    private func launchOnAgentTerminal() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        app.buttons["Agent"].firstMatch.tap()
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        return app
    }
}
