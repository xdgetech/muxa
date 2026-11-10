import XCTest

/// A workspace's assistant on Work items (ADR 0048), against the demo's fixtures:
/// w-0000b001 has a Result of version 2 (version 1's is Earlier), w-0000b002 has none
/// and holds c-0000c003, a Capture sorted elsewhere, and w-0000b003 is in "personal",
/// which takes no requests. A new run is pending, then processing, and has its Result
/// about 8 seconds after it started. `-demo-no-daemon` refuses every request.
final class AssistantTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }

    /// Opens a Work item's detail from the Inbox.
    @MainActor
    private func openWork(_ app: XCUIApplication, _ id: String) {
        let row = workRow(app, id)
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["work-title"].waitForExistence(timeout: 5))
    }

    /// Types a capture and asks the assistant in the "work" workspace.
    @MainActor
    private func askFromCapture(_ app: XCUIApplication, _ text: String) {
        tapWhenReady(app.buttons["sidebar-capture"])
        let field = element(app, "capture-text")
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        if !waitForFocus(field) { field.tap() }
        field.typeText(text)
        tapWhenReady(element(app, "capture-workspace"))
        tapWhenReady(app.buttons["work"].firstMatch)
        tapWhenReady(app.buttons["capture-ask"])
    }

    /// Ask Assistant in the Capture sheet makes a Work item whose row shows the
    /// delivery, then that a Result is ready.
    @MainActor
    func testAskFromTheCaptureSheet() throws {
        let app = launchWorkInbox()
        askFromCapture(app, "Compare two hosting plans")
        XCTAssertTrue(element(app, "capture-text").waitForNonExistence(timeout: 5), "Ask closes the sheet")
        let row = workRow(app, "w-00000103")
        XCTAssertTrue(waitForSidebarRow(app, row))
        XCTAssertTrue(row.label.contains("Compare two hosting plans"), row.label)
        XCTAssertTrue(row.label.contains("Assistant:"), row.label)
        let ready = expectation(for: NSPredicate(format: "label CONTAINS 'Result ready'"), evaluatedWith: row)
        wait(for: [ready], timeout: 20)
        save("assistant-row")
    }

    /// Without the daemon, the host refuses: its message shows, and the Capture is kept
    /// in Unsorted.
    @MainActor
    func testARefusalShowsTheHostsMessageAndKeepsTheCapture() throws {
        let app = launchWorkInbox(["-demo-no-daemon"])
        askFromCapture(app, "Book a table")
        let error = element(app, "capture-error")
        XCTAssertTrue(error.waitForExistence(timeout: 5))
        XCTAssertTrue(error.label.contains("Automatic processing is off"), error.label)
        tapWhenReady(app.buttons["capture-done"])
        let kept = captureRow(app, "c-00000103")
        XCTAssertTrue(waitForSidebarRow(app, kept))
        XCTAssertTrue(kept.label.contains("Book a table"), kept.label)
    }

    /// A Result shows its summary, findings with their sources, open questions, files
    /// as paths, possible duplicates, and the older version under Earlier. Once read,
    /// the row no longer says it is ready.
    @MainActor
    func testAResultIsShown() throws {
        let app = launchWorkInbox()
        let row = workRow(app, "w-0000b001")
        XCTAssertTrue(waitForSidebarRow(app, row))
        XCTAssertTrue(row.label.contains("Result ready"), row.label)
        openWork(app, "w-0000b001")
        let summary = app.staticTexts["work-result-summary"]
        XCTAssertTrue(summary.waitForExistence(timeout: 5))
        XCTAssertTrue(summary.label.hasPrefix("Two ways fit."), summary.label)
        XCTAssertTrue(element(app, "work-request-q-0000d001").label.contains("version 2"))
        let link = app.links["Migrating billing"]
        scrollWorkDetail(app, to: link)
        XCTAssertTrue(link.exists, "a finding's source opens as a link")
        let question = app.staticTexts["work-result-question-0"]
        scrollWorkDetail(app, to: question)
        XCTAssertTrue(question.label.contains("open invoices"), question.label)
        let path = app.staticTexts["/work/assistant/research-notes/plan-mapping.md"]
        scrollWorkDetail(app, to: path)
        XCTAssertTrue(path.exists, "a file is a path")
        let duplicate = element(app, "work-duplicate-w-0000b002")
        scrollWorkDetail(app, to: duplicate)
        XCTAssertTrue(duplicate.exists)
        let earlier = element(app, "work-result-earlier")
        scrollWorkDetail(app, to: earlier)
        XCTAssertTrue(earlier.exists)
        XCTAssertFalse(app.staticTexts["One way fits: move everything at once."].exists, "Earlier starts collapsed")
        save("assistant-result")

        showInboxList(app)
        XCTAssertTrue(waitForSidebarRow(app, row))
        let read = expectation(for: NSPredicate(format: "NOT (label CONTAINS 'Result ready')"), evaluatedWith: row)
        wait(for: [read], timeout: 5)
    }

    /// `get-work` shows every Capture of the item, also one sorted on another device.
    @MainActor
    func testTheDetailShowsCapturesSortedElsewhere() throws {
        let app = launchWorkInbox()
        openWork(app, "w-0000b002")
        let text = element(app, "work-capture-c-0000c003")
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        XCTAssertTrue(text.label.contains("conditional UI flag"), text.label)
        XCTAssertFalse(app.staticTexts["1 more capture"].exists)
    }

    /// Ask Assistant from the detail: the delivery shows with Cancel and Open Session
    /// while the run lives, then the Result.
    @MainActor
    func testAskFromTheWorkDetail() throws {
        let app = launchWorkInbox()
        openWork(app, "w-0000b002")
        let ask = app.buttons["work-ask"]
        scrollWorkDetail(app, to: ask)
        XCTAssertEqual(ask.label, "Ask Assistant")
        tapWhenReady(app.buttons["Draft"].firstMatch)
        ask.tap()
        let delivery = element(app, "work-delivery")
        XCTAssertTrue(delivery.waitForExistence(timeout: 5))
        XCTAssertTrue(["pending", "received", "processing"].contains { delivery.label.contains($0) }, delivery.label)
        XCTAssertTrue(app.buttons["work-cancel"].exists)
        XCTAssertTrue(app.buttons["work-open-session"].waitForExistence(timeout: 5))
        save("assistant-delivery")
        let summary = app.staticTexts["work-result-summary"]
        XCTAssertTrue(summary.waitForExistence(timeout: 20))
        XCTAssertFalse(app.buttons["work-cancel"].exists)
        XCTAssertFalse(app.buttons["work-open-session"].exists, "the run's session closed with its Result")
        let again = app.buttons["work-ask"]
        scrollWorkDetail(app, to: again)
        XCTAssertEqual(again.label, "Ask Again")
    }

    /// Cancel stops the run: the delivery is cancelled, and the item can be asked again.
    @MainActor
    func testCancelARequest() throws {
        let app = launchWorkInbox()
        openWork(app, "w-0000b002")
        let ask = app.buttons["work-ask"]
        scrollWorkDetail(app, to: ask)
        ask.tap()
        let cancel = app.buttons["work-cancel"]
        XCTAssertTrue(cancel.waitForExistence(timeout: 5))
        cancel.tap()
        let delivery = element(app, "work-delivery")
        let cancelled = expectation(for: NSPredicate(format: "label CONTAINS 'cancelled'"), evaluatedWith: delivery)
        wait(for: [cancelled], timeout: 5)
        XCTAssertFalse(cancel.exists)
        let again = app.buttons["work-ask"]
        scrollWorkDetail(app, to: again)
        XCTAssertTrue(again.waitForExistence(timeout: 5))
        XCTAssertEqual(again.label, "Ask Again")
    }

    /// A workspace with no assistant is refused with the host's message.
    @MainActor
    func testARefusalInTheDetailShowsTheHostsMessage() throws {
        let app = launchWorkInbox()
        openWork(app, "w-0000b003")
        let ask = app.buttons["work-ask"]
        scrollWorkDetail(app, to: ask)
        ask.tap()
        let error = element(app, "work-ask-error")
        XCTAssertTrue(error.waitForExistence(timeout: 5))
        XCTAssertTrue(error.label.contains("takes no assistant requests"), error.label)
    }
}
