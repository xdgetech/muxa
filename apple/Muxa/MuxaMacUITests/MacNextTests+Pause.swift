import XCTest

/// Pause and resume a Task (ADR 0104), against the demo: `t10` ships paused.
extension MacNextTests {
    @MainActor
    func testASidebarRowPausesAndResumesItsTask() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        let row = any["terminal.t1"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.rightClick()
        let pause = app.menuItems["Pause Task"]
        XCTAssertTrue(pause.waitForExistence(timeout: 5))
        pause.click()
        XCTAssertTrue(waitFor(row, labelContaining: "paused"), "the row stays, marked paused")
        row.click()
        XCTAssertTrue(any["paused.view"].waitForExistence(timeout: 5))
        row.rightClick()
        let resume = app.menuItems["Resume Task"]
        XCTAssertTrue(resume.waitForExistence(timeout: 5))
        XCTAssertTrue(app.menuItems["Close Task…"].exists, "a paused Task closes too")
        resume.click()
        XCTAssertTrue(any["focus.pane"].waitForExistence(timeout: 10), "Resume opens the Task")
        XCTAssertTrue(waitForGone(row))
        let again = any.matching(NSPredicate(format: "identifier BEGINSWITH 'terminal.' AND label CONTAINS 'fix-login'"))
            .firstMatch
        XCTAssertTrue(again.exists, "a new agent, with the same title")
        XCTAssertFalse(again.label.contains("paused"))
    }

    @MainActor
    func testAPausedTaskResumesFromItsPausedView() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        let row = any["terminal.t10"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(row.label.contains("paused"))
        row.click()
        let title = app.staticTexts["paused.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        XCTAssertEqual(title.value as? String ?? title.label, "Wait for the API review")
        app.buttons["paused.resume"].click()
        XCTAssertTrue(any["focus.pane"].waitForExistence(timeout: 10))
        XCTAssertTrue(waitForGone(row))
        XCTAssertFalse(title.exists)
    }

    @MainActor
    func testThePaneMenuPausesTheFocusedTask() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["focus.pane"].waitForExistence(timeout: 10))
        // Focus opens on the Task that needs the user: docs-audit, t2.
        app.menuBars.menuItems["Pause Task"].click()
        XCTAssertTrue(any["paused.view"].waitForExistence(timeout: 5), "the Task stays open, paused")
        XCTAssertTrue(waitFor(any["terminal.t2"], labelContaining: "paused"))
        XCTAssertFalse(any["focus.pane"].exists)
    }
}
