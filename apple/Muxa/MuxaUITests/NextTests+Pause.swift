import XCTest

/// Pause and resume a Task (ADR 0104), against the demo: `t10` ships paused.
extension NextTests {
    @MainActor
    func testATaskPausesAndResumesFromItsRowMenu() throws {
        let app = launch()
        showTasks(app)
        let any = app.descendants(matching: .any)
        let row = any["terminal.t1"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.press(forDuration: 1.2)
        let pause = app.buttons["Pause Task"]
        XCTAssertTrue(pause.waitForExistence(timeout: 5))
        pause.tap()
        XCTAssertTrue(waitFor(row, valueIs: "paused"), "the row stays, marked paused")
        row.press(forDuration: 1.2)
        let resume = app.buttons["Resume Task"]
        XCTAssertTrue(resume.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["Pause Task"].exists)
        resume.tap()
        // Resume opens the Task on its new agent.
        let pane = any["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Resuming fix-login"))
        leaveTaskOnPhone(app)
        let again = any.matching(NSPredicate(format: "identifier BEGINSWITH 'terminal.' AND label CONTAINS 'fix-login'"))
            .firstMatch
        XCTAssertTrue(again.waitForExistence(timeout: 5))
        XCTAssertNotEqual(again.identifier, "terminal.t1", "a new agent, with the same title")
        XCTAssertNotEqual(again.value as? String, "paused")
    }

    @MainActor
    func testAPausedTaskOpensOnItsPausedViewAndResumes() throws {
        let app = launch()
        showTasks(app)
        let row = app.descendants(matching: .any)["terminal.t10"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertEqual(row.value as? String, "paused")
        row.tap()
        let title = app.staticTexts["paused.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        XCTAssertEqual(title.label, "Wait for the API review")
        app.buttons["paused.resume"].tap()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Resuming Wait for the API review"))
        XCTAssertFalse(app.staticTexts["paused.title"].exists)
    }

    @MainActor
    func testAPausedTaskClosesAfterAsking() throws {
        let app = launch()
        showTasks(app)
        let row = app.descendants(matching: .any)["terminal.t10"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        let close = app.buttons["paused.close"]
        XCTAssertTrue(close.waitForExistence(timeout: 5))
        close.tap()
        let confirm = app.alerts.buttons["closetask.confirm"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        confirm.tap()
        XCTAssertTrue(waitForGone(app.staticTexts["paused.title"]))
        leaveTaskOnPhone(app)
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["terminal.t10"]))
    }

    /// A phone's Task screen covers the Workspaces tab; a tablet keeps its sidebar.
    func leaveTaskOnPhone(_ app: XCUIApplication) {
        guard UIDevice.current.userInterfaceIdiom == .phone else { return }
        let back = app.buttons["mode.home"]
        if back.exists { back.tap() }
        XCTAssertTrue(app.tabBars.buttons["Workspaces"].waitForExistence(timeout: 5))
    }

    func waitFor(_ e: XCUIElement, valueIs value: String) -> Bool {
        let p = NSPredicate(format: "value == %@", value)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }
}
