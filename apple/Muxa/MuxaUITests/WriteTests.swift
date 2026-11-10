import XCTest

/// Write proposals on a `work` level 13 host (ADR 0052), against the demo's `-demo-writes`
/// fixtures: `-demo-staff` (see `StaffTests`) with `publisher` as the writer of "work".
/// The researcher's result of q-0000d002 (Work item w-0000b005) proposes i-0000f005, a
/// Slack post with Markdown and a link in it; q-0000d001's version 1 result (w-0000b001),
/// older than its version 2, proposes i-0000f006, an 8 KiB comment of 204 numbered lines.
final class WriteTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launchWrites(_ arguments: [String] = []) -> XCUIApplication {
        launchWorkInbox(["-demo-writes"] + arguments)
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }

    @MainActor
    private func openWork(_ app: XCUIApplication, _ id: String) {
        showInboxList(app)
        let row = workRow(app, id)
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["work-title"].waitForExistence(timeout: 5))
    }

    @MainActor
    private func openItem(_ app: XCUIApplication, _ id: String) {
        showInboxList(app)
        let row = app.buttons["item-row-\(id)"]
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["item-question"].waitForExistence(timeout: 5))
    }

    /// Scrolls the item screen until `target` is on screen.
    @MainActor
    private func revealInItem(_ app: XCUIApplication, _ target: XCUIElement) -> Bool {
        for _ in 0..<8 where !(target.exists && target.isHittable) {
            app.scrollViews.firstMatch.swipeUp()
        }
        return target.exists
    }

    @MainActor
    private func reveal(_ app: XCUIApplication, _ target: XCUIElement) -> Bool {
        scrollWorkDetail(app, to: target)
        return target.exists
    }

    /// The item shows the action, the target, the content as plain text (the Markdown's
    /// marks stay, the link is no link) and the reason, with Approve, Reject and Later.
    @MainActor
    func testAWriteShowsItsContentAsPlainText() throws {
        let app = launchWrites()
        openItem(app, "i-0000f005")
        let question = app.staticTexts["item-question"]
        XCTAssertTrue(question.label.contains("researcher proposes to write to slack:#finance"), question.label)
        XCTAssertTrue(element(app, "item-write-action").label.contains("Post to #finance"))
        XCTAssertTrue(element(app, "item-write-target").label.contains("slack:#finance"))
        let content = element(app, "item-write-content")
        XCTAssertTrue(content.waitForExistence(timeout: 5))
        XCTAssertTrue(content.label.contains("**Why:** it sends all 27 EU formats"), content.label)
        XCTAssertTrue(content.label.contains("[EU formats](https://example.com/eu-invoicing)"), content.label)
        XCTAssertEqual(app.links.count, 0, "no live link")
        XCTAssertTrue(element(app, "item-write-reason").label.contains("Finance asked"))
        XCTAssertFalse(element(app, "item-write-older").exists, "from the current result")
        XCTAssertTrue(app.buttons["item-answer-approve"].exists)
        XCTAssertTrue(app.buttons["item-answer-reject"].exists)
        XCTAssertTrue(app.buttons["item-answer-later"].exists)
        save("item-write")
    }

    /// Every line of an 8 KiB content is on the screen, and an older result is marked.
    @MainActor
    func testTheWholeContentOfAnOlderResultShows() throws {
        let app = launchWrites()
        openItem(app, "i-0000f006")
        let content = element(app, "item-write-content")
        XCTAssertTrue(content.waitForExistence(timeout: 5))
        let lines = content.label.components(separatedBy: "\n").filter { $0.hasPrefix("Line ") }
        XCTAssertEqual(lines.count, 204, "every line of the 8 KiB fixture")
        XCTAssertEqual(lines.first, "Line 001: keep the old invoice numbers.")
        XCTAssertEqual(lines.last, "Line 204: keep the old invoice numbers.")
        XCTAssertTrue(revealInItem(app, element(app, "item-write-older")))
        XCTAssertTrue(element(app, "item-write-older").label.contains("older result"))
    }

    /// Approve names the target once more; the writer's Task starts once and opens from
    /// Work detail, which says "Task started", never that the write happened.
    @MainActor
    func testApproveConfirmsTheTargetAndStartsTheWritersTask() throws {
        let app = launchWrites()
        openItem(app, "i-0000f005")
        tapWhenReady(app.buttons["item-answer-approve"])
        let confirm = app.buttons["write-confirm"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["The writer writes it to slack:#finance"].exists, "the target once more")
        save("item-write-confirm")
        confirm.tap()
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f005"].waitForNonExistence(timeout: 5), "answered")

        openWork(app, "w-0000b005")
        let task = app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'work-write-task-'")).firstMatch
        XCTAssertTrue(reveal(app, task))
        XCTAssertTrue(task.label.hasPrefix("Task started"), task.label)
        XCTAssertFalse(task.label.localizedCaseInsensitiveContains("posted"))
        save("work-writes")
        task.tap()
        XCTAssertTrue(element(app, "task-detail").waitForExistence(timeout: 5))
    }

    /// Reject offers a reason and needs none; Work detail then shows the write rejected.
    @MainActor
    func testRejectTakesAnOptionalReason() throws {
        let app = launchWrites()
        openItem(app, "i-0000f006")
        tapWhenReady(app.buttons["item-answer-reject"])
        let field = app.alerts.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        if !waitForFocus(field) { field.tap() }
        field.typeText("Not on FIN-12")
        tapWhenReady(app.alerts.buttons["Reject"].firstMatch)
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f006"].waitForNonExistence(timeout: 5))

        openWork(app, "w-0000b001")
        let status = element(app, "work-write-status")
        XCTAssertTrue(reveal(app, status))
        XCTAssertEqual(status.label, "Rejected")
    }

    /// The writer shows under the staff, marked "Writes", with its description; a waiting
    /// write opens its item from Work detail.
    @MainActor
    func testTheWriterShowsWithTheStaff() throws {
        let app = launchWrites()
        openWork(app, "w-0000b005")
        let waiting = app.buttons["work-write-item-i-0000f005"]
        XCTAssertTrue(reveal(app, waiting))
        XCTAssertTrue(waiting.label.contains("Waiting"), waiting.label)
        let writer = element(app, "staff-writer")
        XCTAssertTrue(reveal(app, writer))
        XCTAssertTrue(writer.label.contains("publisher") && writer.label.contains("Writes"), writer.label)
        XCTAssertTrue(writer.label.contains("reads no web pages"), writer.label)
        // The footer under the row: the list makes it once it scrolls into view.
        let mode = element(app, "staff-writes")
        XCTAssertTrue(reveal(app, mode))
        XCTAssertTrue(mode.label.contains("approve each one"), mode.label)
        save("staff-writer")
        // Back up to the waiting write: it opens its item.
        for _ in 0..<8 where !(waiting.exists && waiting.isHittable) { app.collectionViews["work-detail"].swipeDown() }
        tapWhenReady(waiting)
        XCTAssertTrue(element(app, "item-write-content").waitForExistence(timeout: 5))
    }

    /// A level 12 host shows none of it: no write items, no writes, no writer.
    @MainActor
    func testALevel12HostShowsNoWrites() throws {
        let app = launchWrites(["-work12"])
        XCTAssertFalse(app.buttons["item-row-i-0000f005"].exists, "no write items")
        openWork(app, "w-0000b005")
        XCTAssertTrue(reveal(app, app.buttons["work-ask"]))
        XCTAssertFalse(element(app, "work-write").exists)
        XCTAssertFalse(element(app, "staff-writer").exists)
    }
}
