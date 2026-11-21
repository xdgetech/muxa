import XCTest

/// The Mac app against its in-app v2 demo server (`-demo`), as the iOS `NextTests`.
final class MacNextTests: XCTestCase {
    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    func testHomeShowsTheInboxAndWhoNeedsYou() throws {
        let app = launch()
        for id in ["d1", "d2", "d3", "d4", "d5", "d6"] {
            XCTAssertTrue(app.descendants(matching: .any)["item.\(id)"].exists, id)
        }
        XCTAssertTrue(app.buttons["needsyou.t2"].exists)
    }

    @MainActor
    func testACaptureGoesOnTop() throws {
        let app = launch()
        let field = app.textFields["capture.field"]
        field.click()
        field.typeText("reply Sam about the launch #personal due today")
        XCTAssertTrue(app.buttons["#Personal, remove"].waitForExistence(timeout: 5))
        app.buttons["capture.add"].click()
        let top = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'item.'")).firstMatch
        XCTAssertTrue(waitFor(top, labelContaining: "Sam about the launch"))
    }

    @MainActor
    func testApproveTakesTheReviewOutOfTheList() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d1"].click()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        app.buttons["detail.main"].click()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
    }

    @MainActor
    func testTypingInFocusReachesTheTerminal() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        pane.click()
        pane.typeText("y")
        // The demo's program echoes what it gets.
        XCTAssertTrue(waitFor(pane, valueContaining: "(y/n) y"))
    }

    private func waitFor(_ e: XCUIElement, valueContaining text: String) -> Bool {
        let p = NSPredicate(format: "value CONTAINS %@", text)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }

    private func waitFor(_ e: XCUIElement, labelContaining text: String) -> Bool {
        let p = NSPredicate(format: "label CONTAINS %@", text)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }

    private func waitForGone(_ e: XCUIElement) -> Bool {
        let p = NSPredicate(format: "exists == false")
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }
}
