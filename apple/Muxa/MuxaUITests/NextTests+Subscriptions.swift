import XCTest

/// Subscriptions and Usage (ADR 0091) against the demo: app's env login is
/// found and adopted, and each subscription shows its last report.
extension NextTests {
    @MainActor
    func testAFoundLoginBecomesASubscription() throws {
        let app = launch()
        openSettingsRow(app, "settings.subscriptions")
        let add = app.buttons["subs.found.add.claude-app"]
        XCTAssertTrue(add.waitForExistence(timeout: 10), "app's env login is offered")
        add.tap()
        XCTAssertTrue(app.descendants(matching: .any)["sub.claude-app"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["subs.found.add.claude-app"].exists, "nothing left to adopt")
        let personal = app.descendants(matching: .any)["sub.claude-personal"]
        XCTAssertTrue(waitFor(personal, labelContaining: "82% of 5 h"))
    }

    /// ADR 0091 §4: Use makes a login Current for the next Tasks; switching
    /// when full is the Workspace's choice.
    @MainActor
    func testUseMakesASubscriptionCurrentForNextTasks() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.agent", name: "Personal")
        let use = app.buttons["ws.subs.use.claude-spare"]
        scrollTo(use, in: app)
        XCTAssertTrue(app.descendants(matching: .any)["ws.subs.current.claude-personal"].exists)
        use.tap()
        XCTAssertTrue(app.descendants(matching: .any)["ws.subs.current.claude-spare"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["ws.subs.use.claude-personal"].exists, "claude-personal is no longer Current")
        let toggle = app.switches["ws.subs.switch"]
        scrollTo(toggle, in: app)
        XCTAssertEqual(toggle.value as? String, "1")
        toggle.switches.firstMatch.tap()
        XCTAssertTrue(waitFor(toggle, valueContaining: "0"))
    }

    @MainActor
    func testUsageShowsEachSubscriptionsLastReport() throws {
        let app = launch()
        openSettingsRow(app, "settings.usage")
        let personal = app.descendants(matching: .any)["usage.claude-personal"]
        XCTAssertTrue(personal.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["82%"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["No data yet. It shows after the first agent on this login answers."].exists,
                      "claude-spare has not run yet")
        // Today's tokens: a Task whose agent still runs goes by its name.
        let task = app.descendants(matching: .any)["usage.task.app-fix-login"]
        app.swipeUp()
        XCTAssertTrue(task.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["fix-login"].exists)
        // A row's name and value may read as one element.
        let tokens = NSPredicate(format: "label CONTAINS %@", "1.2M in · 84k out · 9.4M cached")
        XCTAssertTrue(app.descendants(matching: .any).matching(tokens).firstMatch.waitForExistence(timeout: 5), "app's tokens today")
        // Its row opens the Task.
        task.tap()
        XCTAssertTrue(app.buttons["task.terminal.t1"].waitForExistence(timeout: 10), "fix-login's Task is open")
    }

    @MainActor
    func testStartTaskNamesTheSubscriptionItRunsOn() throws {
        let app = launch()
        // app lists no subscription until its env login is adopted.
        openSettingsRow(app, "settings.subscriptions")
        let add = app.buttons["subs.found.add.claude-app"]
        XCTAssertTrue(add.waitForExistence(timeout: 10))
        add.tap()
        XCTAssertTrue(app.descendants(matching: .any)["sub.claude-app"].waitForExistence(timeout: 5))
        showTasks(app)
        app.buttons["home.newtask"].tap()
        XCTAssertTrue(app.textFields["task.title"].waitForExistence(timeout: 10))
        let line = app.staticTexts["task.subscription"]
        scrollTo(line, in: app)
        XCTAssertEqual(line.label, "Runs on claude-app · no data yet")
    }

    /// Settings, then a row of its list, scrolled into view first.
    func openSettingsRow(_ app: XCUIApplication, _ id: String) {
        showSettings(app)
        let list = app.descendants(matching: .any)["settings.list"].firstMatch
        XCTAssertTrue(list.waitForExistence(timeout: 10))
        let row = app.buttons[id]
        for _ in 0..<5 where !(row.exists && row.isHittable) { list.swipeUp() }
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.tap()
    }
}
