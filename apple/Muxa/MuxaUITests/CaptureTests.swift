import XCTest

/// Captures and Work items on a `work` level 8 host (ADR 0046), against the demo's
/// fixtures: Work items w-0000b001 to b003, unsorted Captures c-0000c001 and c002, and
/// c-0000c003 sorted into b002. The demo numbers new Captures from c-00000103 and new
/// Work items from w-00000103.
final class CaptureTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
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
        let app = launchWorkInbox()
        let decisions = inboxSection(app, "decisions")
        let work = inboxSection(app, "work")
        let unsorted = inboxSection(app, "unsorted")
        XCTAssertTrue(decisions.exists)
        revealInSidebar(app, captureRow(app, "c-0000c002"))
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
        let app = launchWorkInbox()
        let before = badge(app)
        capture(app, "Call the bank\nAbout the new card")
        let row = captureRow(app, "c-00000103")
        XCTAssertTrue(waitForSidebarRow(app, row))
        XCTAssertTrue(row.label.contains("Call the bank"), row.label)
        XCTAssertEqual(badge(app), before)
    }

    /// Make a Work Item turns the Capture into a Work item and shows it; Note keeps it
    /// with no Work item. Both leave Unsorted.
    @MainActor
    func testSortingMovesCapturesOutOfUnsorted() throws {
        let app = launchWorkInbox()
        let flaky = captureRow(app, "c-0000c001")
        revealInSidebar(app, flaky)
        tapWhenReady(flaky)
        XCTAssertTrue(app.staticTexts["capture-full-text"].waitForExistence(timeout: 5))
        tapWhenReady(app.buttons["capture-make-work"])
        let title = app.staticTexts["work-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        XCTAssertEqual(title.label, "Look into the flaky CI job")
        showInboxList(app)
        XCTAssertTrue(workRow(app, "w-00000103").waitForExistence(timeout: 5))
        XCTAssertFalse(flaky.exists)

        let idea = captureRow(app, "c-0000c002")
        revealInSidebar(app, idea)
        swipe(app, idea, right: false, action: "Note")
        XCTAssertTrue(idea.waitForNonExistence(timeout: 5))
    }

    /// Later moves a Work item below the others and keeps it; Complete removes it.
    @MainActor
    func testLaterAndCompleteBySwipe() throws {
        let app = launchWorkInbox()
        let first = workRow(app, "w-0000b001")
        revealInSidebar(app, workRow(app, "w-0000b003"))
        XCTAssertNotEqual(lowestWorkRow(app), first.identifier)
        swipe(app, first, right: false, action: "Later")
        let moved = expectation(for: NSPredicate { _, _ in self.lowestWorkRow(app) == first.identifier }, evaluatedWith: nil)
        wait(for: [moved], timeout: 5)
        XCTAssertTrue(first.label.contains("later"), first.label)

        let domain = workRow(app, "w-0000b003")
        revealInSidebar(app, domain)
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
        let app = launchWorkInbox()
        let row = workRow(app, "w-0000b001")
        revealInSidebar(app, row)
        tapWhenReady(row)
        scrollWorkDetail(app, to: app.buttons["work-start-task"])
        tapWhenReady(app.buttons["work-start-task"])
        let title = app.textFields["record-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        XCTAssertEqual(title.value as? String, "Plan the billing migration")
        tapWhenReady(app.buttons["record-start"])
        XCTAssertTrue(title.waitForNonExistence(timeout: 10))
        // Its session opens; the Work item is in progress now.
        XCTAssertTrue(app.descendants(matching: .any)["pane"].firstMatch.waitForExistence(timeout: 10))
        showSidebar(app)
        revealInSidebar(app, row)
        let progress = expectation(for: NSPredicate(format: "label CONTAINS 'in progress'"), evaluatedWith: row)
        wait(for: [progress], timeout: 5)
    }

    /// A capture saved while the connection drops stays on the device as Not sent, and
    /// is sent when the app connects again.
    @MainActor
    func testACaptureMadeOfflineIsSentOnConnect() throws {
        let app = launchWorkInbox(["-demo-drop-capture"])
        capture(app, "Buy stamps")
        let unsent = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'unsent-row-'")).firstMatch
        XCTAssertTrue(unsent.waitForExistence(timeout: 10))
        XCTAssertTrue(unsent.label.contains("Not sent"), unsent.label)
        tapWhenReady(app.buttons["connect-last"])
        let row = captureRow(app, "c-00000103")
        XCTAssertTrue(app.segmentedControls["sidebar-mode"].waitForExistence(timeout: 10), "connected again")
        XCTAssertTrue(waitForSidebarRow(app, row))
        XCTAssertTrue(row.label.contains("Buy stamps"), row.label)
        XCTAssertTrue(unsent.waitForNonExistence(timeout: 5))
    }

    /// A host before `work` level 8 keeps New Task behind `+`, and its Inbox has no Work.
    @MainActor
    func testAnOlderHostKeepsNewTask() throws {
        let app = launchWorkInbox(["-work7"])
        XCTAssertFalse(app.buttons["sidebar-capture"].exists)
        XCTAssertFalse(inboxSection(app, "unsorted").exists)
        XCTAssertFalse(inboxSection(app, "decisions").exists, "the Inbox as before")
        XCTAssertFalse(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'work-row-'")).firstMatch.exists)
        tapWhenReady(app.buttons["sidebar-new-task"])
        XCTAssertTrue(app.textFields["record-title"].waitForExistence(timeout: 5))
    }
}
