import XCTest

/// Dragging down in a pane scrolls into the host's earlier lines in place, not in a sheet.
/// The demo's "device" session (%9) follows the device (ADR 0020): its grid fits the view
/// and has nothing of its own to scroll, as on a host since then.
final class ScrollbackTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    @MainActor
    func testADragDownShowsEarlierLinesInPlace() throws {
        let app = launch()
        let pane = openDeviceSession(app)

        dragDown(pane)
        let history = app.descendants(matching: .any)["pane-history"].firstMatch
        XCTAssertTrue(history.waitForExistence(timeout: 5), "the drag showed no earlier lines in place")
        // The rest of the same drag already read back past the newest line.
        let shown = history.value as? String ?? ""
        XCTAssertTrue(shown.contains("Earlier output, line 1"), shown)
        XCTAssertFalse(shown.contains("Earlier output, line 200"), "the drag stopped at the newest line")
        XCTAssertFalse(app.navigationBars["Scrollback"].exists, "the earlier lines opened in a sheet")
        save("scrollback-in-place")
    }

    /// Output while the user reads back does not move what they read.
    @MainActor
    func testOutputDoesNotMoveTheEarlierLines() throws {
        let app = launch(["-ticker"])
        let pane = openDeviceSession(app)
        dragDown(pane)
        dragDown(pane)
        let history = app.descendants(matching: .any)["pane-history"].firstMatch
        XCTAssertTrue(history.waitForExistence(timeout: 5))
        let before = history.value as? String ?? ""
        XCTAssertFalse(before.contains("line 200"), "the drags did not scroll back")
        let screen = pane.value as? String
        sleep(3)
        XCTAssertNotEqual(pane.value as? String, screen, "the demo printed no output")
        XCTAssertEqual(history.value as? String, before, "output moved the earlier lines")
        save("scrollback-kept-while-output")
    }

    /// Live returns to the pane; a drag past the newest line does too.
    @MainActor
    func testLiveAndTheNewestLineReturnToThePane() throws {
        let app = launch()
        let pane = openDeviceSession(app)
        let history = app.descendants(matching: .any)["pane-history"].firstMatch

        dragDown(pane)
        XCTAssertTrue(history.waitForExistence(timeout: 5))
        tapWhenReady(app.buttons["pane-live"])
        XCTAssertTrue(history.waitForNonExistence(timeout: 5), "Live did not return to the pane")

        dragDown(pane)
        XCTAssertTrue(history.waitForExistence(timeout: 5))
        for _ in 0..<3 where history.exists { history.swipeUp(velocity: .fast) }
        XCTAssertTrue(history.waitForNonExistence(timeout: 5), "the newest line did not return to the pane")
        XCTAssertTrue(pane.isHittable)

        // Typing while reading back returns to the live pane and reaches it.
        dragDown(pane)
        XCTAssertTrue(history.waitForExistence(timeout: 5))
        pane.typeText("hello")
        XCTAssertTrue(history.waitForNonExistence(timeout: 5), "a key did not return to the pane")
        XCTAssertTrue(waitFor(pane, containing: "hello"))
    }

    @MainActor
    private func launch(_ extra: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "sessions"] + extra
        app.launch()
        return app
    }

    /// The "device" session: Cmd+4 in the iPad sidebar, More, Sessions… on a phone.
    @MainActor
    private func openDeviceSession(_ app: XCUIApplication) -> XCUIElement {
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        if UIDevice.current.userInterfaceIdiom == .pad {
            pane.typeKey("4", modifierFlags: .command)
        } else {
            tapWhenReady(app.buttons["More"])
            tapWhenReady(app.buttons["Sessions…"])
            tapWhenReady(app.buttons.containing(NSPredicate(format: "label BEGINSWITH 'device'")).firstMatch)
        }
        // Longer than `waitFor`: with `-ticker` the app is seldom idle for a read of its text.
        let title = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value CONTAINS 'pane %9 '"), object: pane)
        XCTAssertEqual(XCTWaiter.wait(for: [title], timeout: 20), .completed, "the device session did not open")
        return pane
    }

    /// A slow drag, as a finger reads: a flick would end before a reply could arrive.
    private func dragDown(_ element: XCUIElement) {
        let start = element.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.25))
        let end = element.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75))
        start.press(forDuration: 0.1, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0.3)
    }
}
