import XCTest

/// Captures and Work items on a `work` level 8 host (ADR 0046), against the demo's
/// fixtures: Work items w-0000b001 to b003, unsorted Captures c-0000c001 and c002.
/// The demo numbers new Captures from c-00000102 and new Work items from w-00000103.
final class CaptureTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launchOnInbox(_ arguments: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "inbox"] + arguments
        app.launch()
        XCTAssertTrue(app.buttons["item-row-i-0000a001"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    private func workRow(_ app: XCUIApplication, _ id: String) -> XCUIElement { app.buttons["work-row-\(id)"] }

    @MainActor
    private func section(_ app: XCUIApplication, _ name: String) -> XCUIElement {
        app.descendants(matching: .any)["inbox-section-\(name)"].firstMatch
    }

    @MainActor
    private func captureRow(_ app: XCUIApplication, _ id: String) -> XCUIElement { app.buttons["capture-row-\(id)"] }

    /// Scrolls the sidebar until `element` is clear of the screen's bottom edge, where a
    /// drag would reach the home indicator instead of the row.
    @MainActor
    private func reveal(_ app: XCUIApplication, _ element: XCUIElement) {
        let bottom = app.windows.firstMatch.frame.maxY - 80
        for _ in 0..<4 where !(element.isHittable && element.frame.maxY < bottom) {
            app.collectionViews["sidebar-list"].swipeUp()
        }
    }

    /// Waits for a row that is still to come, scrolling: a phone's list makes rows
    /// below the screen only once they scroll into view.
    @MainActor
    private func waitForRow(_ app: XCUIApplication, _ row: XCUIElement) -> Bool {
        for _ in 0..<5 {
            if row.waitForExistence(timeout: 2) {
                reveal(app, row)
                return true
            }
            app.collectionViews["sidebar-list"].swipeUp()
        }
        return row.exists
    }

    /// The Inbox segment's label, which counts the decisions.
    @MainActor
    private func badge(_ app: XCUIApplication) -> String {
        app.segmentedControls["sidebar-mode"].buttons.element(boundBy: 0).label
    }

    @MainActor
    private func capture(_ app: XCUIApplication, _ text: String) {
        tapWhenReady(app.buttons["sidebar-capture"])
        let field = app.descendants(matching: .any)["capture-text"].firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        if !waitForFocus(field) { field.tap() }
        field.typeText(text)
        tapWhenReady(app.buttons["capture-save"])
        XCTAssertTrue(field.waitForNonExistence(timeout: 5), "Save closes the sheet")
    }

    /// A finger's drag across a row: `swipeLeft()` is too short for a sidebar row. A
    /// shorter drag shows the action's button instead of running it; then it is tapped.
    @MainActor
    private func swipe(_ app: XCUIApplication, _ row: XCUIElement, right: Bool, action: String) {
        let (from, to) = right ? (0.05, 0.95) : (0.9, 0.05)
        row.coordinate(withNormalizedOffset: CGVector(dx: from, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: row.coordinate(withNormalizedOffset: CGVector(dx: to, dy: 0.5)))
        let button = app.collectionViews["sidebar-list"].buttons[action]
        if button.waitForExistence(timeout: 2), button.isHittable { button.tap() }
    }

    /// The Inbox lists Decisions, then Work, then Unsorted. A Work item whose Task waits
    /// on an answer says so, with no answer of its own.
    @MainActor
    func testTheInboxHasDecisionsWorkAndUnsorted() throws {
        let app = launchOnInbox()
        let decisions = section(app, "decisions")
        let work = section(app, "work")
        let unsorted = section(app, "unsorted")
        XCTAssertTrue(decisions.exists)
        reveal(app, captureRow(app, "c-0000c002"))
        XCTAssertTrue(work.exists)
        XCTAssertTrue(unsorted.exists)
        XCTAssertLessThan(work.frame.minY, unsorted.frame.minY)
        let waiting = workRow(app, "w-0000b002")
        XCTAssertTrue(waiting.label.contains("waiting on decision"), waiting.label)
        XCTAssertTrue(captureRow(app, "c-0000c001").label.contains("Look into the flaky CI job"))
        XCTAssertFalse(captureRow(app, "c-0000c001").label.contains("failed twice"), "the first line only")
        save("inbox-work")
    }

    /// `+` saves a Capture into Unsorted. The badge counts decisions only.
    @MainActor
    func testACaptureIsSavedInUnsorted() throws {
        let app = launchOnInbox()
        let before = badge(app)
        capture(app, "Call the bank\nAbout the new card")
        let row = captureRow(app, "c-00000102")
        XCTAssertTrue(waitForRow(app, row))
        XCTAssertTrue(row.label.contains("Call the bank"), row.label)
        XCTAssertEqual(badge(app), before)
    }

    /// Make a Work Item turns the Capture into a Work item and shows it; Note keeps it
    /// with no Work item. Both leave Unsorted.
    @MainActor
    func testSortingMovesCapturesOutOfUnsorted() throws {
        let app = launchOnInbox()
        let flaky = captureRow(app, "c-0000c001")
        reveal(app, flaky)
        tapWhenReady(flaky)
        XCTAssertTrue(app.staticTexts["capture-full-text"].waitForExistence(timeout: 5))
        tapWhenReady(app.buttons["capture-make-work"])
        let title = app.staticTexts["work-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        XCTAssertEqual(title.label, "Look into the flaky CI job")
        // Back to the list: a phone pushed the detail, a portrait iPad hid the sidebar.
        let modes = app.segmentedControls["sidebar-mode"]
        if !(modes.exists && modes.isHittable) { showSidebar(app) }
        XCTAssertTrue(workRow(app, "w-00000103").waitForExistence(timeout: 5))
        XCTAssertFalse(flaky.exists)

        let idea = captureRow(app, "c-0000c002")
        reveal(app, idea)
        swipe(app, idea, right: false, action: "Note")
        XCTAssertTrue(idea.waitForNonExistence(timeout: 5))
    }

    /// Later moves a Work item below the others and keeps it; Complete removes it.
    @MainActor
    func testLaterAndCompleteBySwipe() throws {
        let app = launchOnInbox()
        let first = workRow(app, "w-0000b001")
        reveal(app, workRow(app, "w-0000b003"))
        XCTAssertNotEqual(lowestWorkRow(app), first.identifier)
        swipe(app, first, right: false, action: "Later")
        let moved = expectation(for: NSPredicate { _, _ in self.lowestWorkRow(app) == first.identifier }, evaluatedWith: nil)
        wait(for: [moved], timeout: 5)
        XCTAssertTrue(first.label.contains("later"), first.label)

        let domain = workRow(app, "w-0000b003")
        reveal(app, domain)
        swipe(app, domain, right: true, action: "Complete")
        XCTAssertTrue(domain.waitForNonExistence(timeout: 5))
    }

    /// The lowest Work row on screen. The query's order is not the list's.
    @MainActor
    private func lowestWorkRow(_ app: XCUIApplication) -> String? {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'work-row-'")).allElementsBoundByIndex
            .max { $0.frame.minY < $1.frame.minY }?.identifier
    }

    /// Start a Task from a Work item opens New Task with its title; the Task is listed on it.
    @MainActor
    func testAWorkItemStartsATask() throws {
        let app = launchOnInbox()
        let row = workRow(app, "w-0000b001")
        reveal(app, row)
        tapWhenReady(row)
        tapWhenReady(app.buttons["work-start-task"])
        let title = app.textFields["record-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        XCTAssertEqual(title.value as? String, "Plan the billing migration")
        tapWhenReady(app.buttons["record-start"])
        XCTAssertTrue(title.waitForNonExistence(timeout: 10))
        // Its session opens; the Work item is in progress now.
        XCTAssertTrue(app.descendants(matching: .any)["pane"].firstMatch.waitForExistence(timeout: 10))
        showSidebar(app)
        reveal(app, row)
        let progress = expectation(for: NSPredicate(format: "label CONTAINS 'in progress'"), evaluatedWith: row)
        wait(for: [progress], timeout: 5)
    }

    /// A capture saved while the connection drops stays on the device as Not sent, and
    /// is sent when the app connects again.
    @MainActor
    func testACaptureMadeOfflineIsSentOnConnect() throws {
        let app = launchOnInbox(["-demo-drop-capture"])
        capture(app, "Buy stamps")
        let unsent = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'unsent-row-'")).firstMatch
        XCTAssertTrue(unsent.waitForExistence(timeout: 10))
        XCTAssertTrue(unsent.label.contains("Not sent"), unsent.label)
        tapWhenReady(app.buttons["connect-last"])
        let row = captureRow(app, "c-00000102")
        XCTAssertTrue(app.segmentedControls["sidebar-mode"].waitForExistence(timeout: 10), "connected again")
        XCTAssertTrue(waitForRow(app, row))
        XCTAssertTrue(row.label.contains("Buy stamps"), row.label)
        XCTAssertTrue(unsent.waitForNonExistence(timeout: 5))
    }

    /// A host before `work` level 8 keeps New Task behind `+`, and its Inbox has no Work.
    @MainActor
    func testAnOlderHostKeepsNewTask() throws {
        let app = launchOnInbox(["-work7"])
        XCTAssertFalse(app.buttons["sidebar-capture"].exists)
        XCTAssertFalse(section(app, "unsorted").exists)
        XCTAssertFalse(section(app, "decisions").exists, "the Inbox as before")
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'work-row-'")).firstMatch.exists)
        tapWhenReady(app.buttons["sidebar-new-task"])
        XCTAssertTrue(app.textFields["record-title"].waitForExistence(timeout: 5))
    }
}
