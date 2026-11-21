import XCTest

/// The app on protocol v2 (`-next`) against the in-app v2 demo server (`-demo`).
final class NextTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-next", "-demo"]
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
    func testACaptureShowsItsFieldsAndGoesOnTop() throws {
        let app = launch()
        let field = app.textFields["capture.field"]
        field.tap()
        field.typeText("reply Sam about the launch #personal due today")
        XCTAssertTrue(app.buttons["#Personal, remove"].waitForExistence(timeout: 5))
        app.buttons["capture.add"].tap()
        let top = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'item.'")).firstMatch
        XCTAssertTrue(waitFor(top, labelContaining: "Sam about the launch"))
        XCTAssertTrue(top.label.contains("Reply"))
    }

    @MainActor
    func testApproveTakesTheReviewOutOfTheList() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d1"].tap()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["detail.title"].label, "Review fix-login")
        app.buttons["detail.main"].tap()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
    }

    @MainActor
    func testARoutineRunsNow() throws {
        let app = launch()
        app.segmentedControls["home.part"].buttons["Routines"].tap()
        let row = app.descendants(matching: .any)["routine.release-notes"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        let run = app.buttons["routine.run"]
        XCTAssertTrue(run.waitForExistence(timeout: 5))
        run.tap()
        // The detail (on a phone it covers the list) shows the run started.
        let started = NSPredicate(format: "isEnabled == false")
        XCTAssertEqual(XCTWaiter().wait(for: [expectation(for: started, evaluatedWith: run)], timeout: 10), .completed)
        XCTAssertTrue(app.staticTexts["Running since"].exists)
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
