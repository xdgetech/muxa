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
    }
}
