import XCTest

/// Folding and unfolding an iPhone Duo, played on an iPad with the demo's `-foldable`
/// button: the size class changes under the running app. Skipped on a phone.
final class FoldTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUpWithError() throws {
        try XCTSkipIf(UIDevice.current.userInterfaceIdiom == .phone, "needs the room of an unfolded screen")
    }

    /// Folded is the phone layout, unfolded the iPad layout. The same pane, its
    /// screen and the keyboard stay through both.
    @MainActor
    func testFoldingKeepsThePaneAndTheKeyboard() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-foldable"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        showAgentTerminal(app)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        tapPane(pane)
        XCTAssertTrue(waitForFocus(pane))
        pane.typeText("one")
        XCTAssertTrue(waitFor(pane, containing: "$ one"))

        let fold = app.buttons["demo-fold"]
        fold.tap()
        XCTAssertEqual(fold.label, "Unfold")
        // The phone layout: a stack with Back, and a readable font wider than the screen.
        XCTAssertTrue(app.navigationBars.buttons["BackButton"].waitForExistence(timeout: 5))
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        XCTAssertTrue(waitFor(pane, containing: "$ one"), "the screen was lost")
        sleep(1)
        XCTAssertGreaterThan(pane.frame.width, 466)
        save("folded")
        XCTAssertTrue(waitForFocus(pane))
        pane.typeText("two")
        XCTAssertTrue(waitFor(pane, containing: "$ onetwo"))

        fold.tap()
        XCTAssertEqual(fold.label, "Fold")
        XCTAssertTrue(app.navigationBars.buttons["BackButton"].waitForNonExistence(timeout: 5))
        XCTAssertTrue(waitFor(pane, containing: "$ onetwo"), "the screen was lost")
        sleep(1)
        // The iPad layout fits the pane in the screen again. The keyboard's height,
        // which can differ from before, sets the exact width.
        XCTAssertLessThanOrEqual(pane.frame.width, app.windows.firstMatch.frame.width + 1)
        save("unfolded")
        XCTAssertTrue(waitForFocus(pane))
        pane.typeText("three")
        XCTAssertTrue(waitFor(pane, containing: "$ onetwothree"))
    }
}
