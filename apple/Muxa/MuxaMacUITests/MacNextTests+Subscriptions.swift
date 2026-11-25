import XCTest

/// Subscriptions and Usage in the Settings window (ADR 0091), as the iOS
/// `NextTests+Subscriptions`.
extension MacNextTests {
    @MainActor
    func testAFoundLoginBecomesASubscription() throws {
        let app = launch()
        openSettings(app)
        let row = app.buttons["settings.subscriptions"]
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()
        let add = app.buttons["subs.found.add.claude-app"]
        XCTAssertTrue(add.waitForExistence(timeout: 10), "app's env login is offered")
        add.click()
        XCTAssertTrue(app.descendants(matching: .any)["sub.claude-app"].waitForExistence(timeout: 5))
        let personal = app.descendants(matching: .any)["sub.claude-personal"]
        XCTAssertTrue(waitFor(personal, labelContaining: "82% of 5 h"))
    }

    @MainActor
    func testUsageShowsEachSubscriptionsLastReport() throws {
        let app = launch()
        openSettings(app)
        let row = app.buttons["settings.usage"]
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()
        XCTAssertTrue(app.descendants(matching: .any)["usage.claude-personal"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["82%"].waitForExistence(timeout: 5))
        // Today's tokens: a Task whose agent still runs goes by its name.
        XCTAssertTrue(app.descendants(matching: .any)["usage.task.app-fix-login"].waitForExistence(timeout: 5))
        // A row's name and value may read as one element.
        let tokens = NSPredicate(format: "label CONTAINS %@", "1.2M in · 84k out · 9.4M cached")
        XCTAssertTrue(app.descendants(matching: .any).matching(tokens).firstMatch.waitForExistence(timeout: 5), "app's tokens today")
    }
}
