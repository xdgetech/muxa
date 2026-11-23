import XCTest

/// A Bot's settings proposal, the Capture assistant's suggestion and settings,
/// and a Routine that runs on review, as the iOS `NextTests`.
extension MacNextTests {
    @MainActor
    func testABotsSettingsProposalIsApplied() throws {
        let app = launch()
        let item = app.descendants(matching: .any)["item.d12"]
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        item.click()
        let op = app.staticTexts["detail.op.0"]
        XCTAssertTrue(op.waitForExistence(timeout: 5))
        XCTAssertEqual(op.value as? String ?? op.label, "routine-add nightly-sweep (off)")
        app.buttons["detail.apply"].click()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d12"]))
        openWorkspace(app, tab: "ws.tab.routines")
        XCTAssertTrue(app.descendants(matching: .any)["settings.routine.nightly-sweep"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testACapturesSuggestionIsAccepted() throws {
        let app = launch()
        let item = app.descendants(matching: .any)["item.d13"]
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        item.click()
        let suggestion = app.descendants(matching: .any)["detail.suggestion"]
        XCTAssertTrue(suggestion.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["suggestion.starttask"].exists)
        app.buttons["suggestion.accept"].click()
        XCTAssertTrue(waitForGone(suggestion))
        XCTAssertTrue(app.descendants(matching: .any)["detail.note"].firstMatch.waitForExistence(timeout: 5))
    }

    @MainActor
    func testARoutineRunsOnReview() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.routines")
        let row = app.descendants(matching: .any)["settings.routine.dependency-digest"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.click()
        let timing = app.popUpButtons["routine.timing"]
        XCTAssertTrue(timing.waitForExistence(timeout: 5))
        timing.click()
        app.menuItems["On review"].click()
        app.buttons["editor.save"].click()
        XCTAssertTrue(waitFor(row, labelContaining: "On review"))
    }

    @MainActor
    func testTheCaptureAssistantIsSaved() throws {
        let app = launch()
        openSettings(app)
        let row = app.descendants(matching: .any)["settings.assistant"]
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()
        XCTAssertTrue(app.descendants(matching: .any)["assistant.enabled"].firstMatch.waitForExistence(timeout: 5))
        let enabled = app.checkBoxes["assistant.enabled"].exists
            ? app.checkBoxes["assistant.enabled"] : app.switches["assistant.enabled"]
        XCTAssertTrue(enabled.waitForExistence(timeout: 5))
        enabled.click()
        app.buttons["assistant.save"].click()
        XCTAssertTrue(app.staticTexts["assistant.saved"].waitForExistence(timeout: 5))
        XCTAssertTrue(waitFor(row, labelContaining: "off"))
    }
}
