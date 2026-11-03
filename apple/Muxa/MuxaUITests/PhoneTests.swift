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

        tapPane(pane)
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

    /// Documents is on the screen, not in More. More is in the bar itself, not folded into a
    /// menu of iOS's, and has Sessions: the list holds manual sessions only (ADR 0027).
    @MainActor
    func testDocumentsIsOnTheScreenAndMoreHasSessions() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        leaveInitialInbox(app)
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        let documents = app.buttons["Documents, 3 changed files"]
        XCTAssertTrue(documents.waitForExistence(timeout: 5))
        XCTAssertTrue(documents.isHittable)
        XCTAssertTrue(app.navigationBars.buttons["More"].isHittable)
        tapWhenReady(app.buttons["More"])
        XCTAssertTrue(app.buttons["Close Task…"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Sessions…"].exists)
        // The one on the screen, none in the menu.
        XCTAssertEqual(app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Documents'")).count, 1)
    }

    /// Documents opens the Reader on its list. Read Plan, on the agent's plan, opens the plan itself.
    @MainActor
    func testTheReaderOpensOnItsListAndReadPlanOnThePlan() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        leaveInitialInbox(app)
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))

        tapWhenReady(app.buttons["Documents, 3 changed files"])
        XCTAssertTrue(app.staticTexts["Reader plan"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.segmentedControls["reader-tab"].isHittable)
        XCTAssertFalse(app.webViews.firstMatch.exists)
        save("phone-reader-list")
        app.buttons["Done"].firstMatch.tap()

        composer.tap()
        composer.typeText("/plan")
        app.buttons["send"].tap()
        let card = app.descendants(matching: .any)["approval"].firstMatch
        XCTAssertTrue(card.waitForExistence(timeout: 5))
        tapButton(card.buttons["approval-read-plan"])
        XCTAssertTrue(app.webViews.firstMatch.staticTexts["Phases"].waitForExistence(timeout: 10))
    }

    /// The demo opens the agent pane in Chat; this shows its terminal.
    @MainActor
    private func launchOnAgentTerminal() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        leaveInitialInbox(app)
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        showAgentTerminal(app)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        return app
    }
}
