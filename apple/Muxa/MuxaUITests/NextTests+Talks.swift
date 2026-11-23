import XCTest

/// A Bot's settings proposal, the Capture assistant's suggestion and settings,
/// and a Routine that runs on review, against the demo (ADR 0083, 0084).
extension NextTests {
    @MainActor
    func testABotsSettingsProposalIsApplied() throws {
        let app = launch()
        let item = inboxItem("d12", in: app)
        item.tap()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        let op = app.staticTexts["detail.op.0"]
        scrollTo(op, in: app)
        XCTAssertEqual(op.label, "routine-add nightly-sweep (off)")
        let apply = app.buttons["detail.apply"]
        scrollTo(apply, in: app)
        apply.tap()
        backToList(app)
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d12"]))
        openWorkspace(app, tab: "ws.tab.routines")
        let row = app.descendants(matching: .any)["settings.routine.nightly-sweep"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(row.label.contains("off"), row.label)
    }

    @MainActor
    func testACapturesSuggestionIsAccepted() throws {
        let app = launch()
        let item = inboxItem("d13", in: app)
        item.tap()
        let suggestion = app.descendants(matching: .any)["detail.suggestion"]
        scrollTo(suggestion, in: app)
        XCTAssertTrue(app.buttons["suggestion.starttask"].exists)
        XCTAssertTrue(app.buttons["suggestion.ask"].label.contains("pr-watch"))
        let accept = app.buttons["suggestion.accept"]
        scrollTo(accept, in: app)
        accept.tap()
        XCTAssertTrue(waitForGone(suggestion))
        let note = app.descendants(matching: .any)["detail.note"].firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 5))
        XCTAssertTrue(note.staticTexts.allElementsBoundByIndex.contains { $0.label.contains("once in ten") })
    }

    @MainActor
    func testARoutineRunsOnReview() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.routines")
        let row = app.descendants(matching: .any)["settings.routine.dependency-digest"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        let timing = app.buttons["routine.timing"]
        XCTAssertTrue(timing.waitForExistence(timeout: 5))
        timing.tap()
        app.buttons["On review"].firstMatch.tap()
        XCTAssertFalse(app.textFields["routine.timezone"].exists, "a Routine on review has no time zone")
        app.buttons["editor.save"].tap()
        XCTAssertTrue(waitFor(row, labelContaining: "On review"))
    }

    @MainActor
    func testTheCaptureAssistantIsSaved() throws {
        let app = launch()
        showSettings(app)
        let row = app.buttons["settings.assistant"]
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        for _ in 0..<4 where !row.isHittable { app.swipeUp() }
        XCTAssertTrue(waitFor(row, labelContaining: "on"))
        row.tap()
        let enabled = app.switches["assistant.enabled"]
        XCTAssertTrue(enabled.waitForExistence(timeout: 5))
        XCTAssertTrue(waitFor(app.staticTexts["assistant.memory"], labelContaining: "Release items go to app"))
        enabled.switches.firstMatch.tap()
        let save = app.buttons["assistant.save"]
        scrollTo(save, in: app)
        save.tap()
        XCTAssertTrue(app.staticTexts["assistant.saved"].waitForExistence(timeout: 5))
        XCTAssertEqual(enabled.value as? String, "0")
    }

    /// An item low in the Inbox: a tablet's list is in the sidebar, beside the
    /// detail, so the swipe goes to the list.
    func inboxItem(_ id: String, in app: XCUIApplication) -> XCUIElement {
        let item = app.descendants(matching: .any)["item.\(id)"]
        let list = app.descendants(matching: .any)["inbox.list"].firstMatch
        for _ in 0..<6 where !(item.exists && item.isHittable) {
            list.swipeUp()
        }
        XCTAssertTrue(item.waitForExistence(timeout: 5))
        return item
    }
}
