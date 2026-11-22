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
    func testTheReaderShowsATasksPlan() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.typeKey("r", modifierFlags: [.command, .shift])
        let plan = app.descendants(matching: .any)["reader.file.PLAN.md"]
        XCTAssertTrue(plan.waitForExistence(timeout: 10))
        plan.click()
        XCTAssertTrue(app.webViews["reader"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testATasksLoopStarts() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.buttons["focus.loop"].click()
        let start = app.buttons["loop.start"]
        XCTAssertTrue(start.waitForExistence(timeout: 5))
        start.click()
        XCTAssertTrue(app.buttons["Loop 0/3"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testChatAnswersThePrompt() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        app.typeKey("c", modifierFlags: [.command, .shift])
        let allow = app.buttons["chat.option.Allow"]
        XCTAssertTrue(allow.waitForExistence(timeout: 10))
        allow.click()
        XCTAssertTrue(waitFor(pane, valueContaining: "(y/n) y"))
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

    @MainActor
    func testGoToTaskAndASecondPane() throws {
        let app = launch()
        app.typeKey("p", modifierFlags: .command)
        let field = app.textFields["goto.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("docs\r")
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        // ⌘\ adds the next Task beside it, and ⌘1 goes back to the first.
        app.typeKey("\\", modifierFlags: .command)
        XCTAssertTrue(app.descendants(matching: .any)["pane.2"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "focus.pane").count, 2)
        app.typeKey("1", modifierFlags: .command)
        app.menuBars.menuItems["Close Pane"].click()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["pane.2"]))
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
