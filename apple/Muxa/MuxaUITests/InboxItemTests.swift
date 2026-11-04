import XCTest

/// The Inbox as a list of questions (ADR 0033) against the demo's items: an ask with
/// a permission, a question, a review with evidence, a failure, a proposal and a briefing.
final class InboxItemTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    /// Opens the demo on the Inbox: a phone opens there by itself.
    @MainActor
    private func launchOnInbox() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        let list = app.descendants(matching: .any)["inbox-list"].firstMatch
        if !list.waitForExistence(timeout: 5) {
            showSidebar(app)
            tapWhenReady(app.buttons["open-tasks"])
        }
        XCTAssertTrue(list.waitForExistence(timeout: 5))
        return app
    }

    @MainActor
    private func row(_ app: XCUIApplication, _ id: String) -> XCUIElement { app.buttons["item-row-\(id)"] }

    @MainActor
    private func answer(_ app: XCUIApplication, _ action: String) -> XCUIElement { app.buttons["item-answer-\(action)"] }

    /// The rows' ids, top to bottom.
    @MainActor
    private func rowIds(_ app: XCUIApplication) -> [String] {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'item-row-'")).allElementsBoundByIndex
            .sorted { $0.frame.minY < $1.frame.minY }
            .map { String($0.identifier.dropFirst("item-row-".count)) }
    }

    /// Opens an item: pushed on a phone, beside the list on a tablet.
    @MainActor
    private func open(_ app: XCUIApplication, _ id: String, question: String) {
        tapWhenReady(row(app, id))
        let shown = app.staticTexts["item-question"]
        let exp = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", question), object: shown)
        XCTAssertEqual(XCTWaiter.wait(for: [exp], timeout: 5), .completed, "\(id) is open")
    }

    /// Asks come first, then the newest; each row has the question, its source and its age.
    @MainActor
    func testTheListShowsItems() throws {
        let app = launchOnInbox()
        XCTAssertTrue(row(app, "i-0000a001").waitForExistence(timeout: 5))
        XCTAssertEqual(Array(rowIds(app).prefix(3)), ["i-0000a001", "i-0000a004", "i-0000a002"])
        XCTAssertTrue(row(app, "i-0000a001").label.contains("Allow Bash: npm run deploy"), row(app, "i-0000a001").label)
        XCTAssertTrue(row(app, "i-0000a001").label.contains("Add sign-in with passkeys · 1m"), row(app, "i-0000a001").label)
        XCTAssertTrue(row(app, "e-000000a2").exists, "a Routine's proposal is an item too")
        XCTAssertFalse(app.buttons["inbox-row-t-auth01"].exists, "the Task list is gone on this host")
        save("inbox-items")
    }

    /// A review shows its summary and evidence; Accept removes it.
    @MainActor
    func testAcceptRemovesAReview() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        XCTAssertTrue(app.staticTexts["item-summary"].label.contains("PR #42"))
        XCTAssertTrue(app.buttons["item-evidence-changes"].exists)
        XCTAssertTrue(app.buttons["item-evidence-doc-docs/design.md"].exists)
        XCTAssertTrue(app.buttons["item-evidence-image-build/screenshots/en/login.png"].exists)
        XCTAssertFalse(answer(app, "change-course").exists, "Change course is under More")
        save("inbox-item-review")
        tapWhenReady(answer(app, "accept"))
        XCTAssertTrue(row(app, "i-0000a002").waitForNonExistence(timeout: 5))
        tapWhenReady(app.segmentedControls["inbox-mode"].buttons["All Tasks"])
        let closed = app.descendants(matching: .any)["inbox-Closed"].firstMatch
        for _ in 0..<3 where !closed.exists { app.collectionViews["all-tasks"].swipeUp() }
        XCTAssertTrue(closed.waitForExistence(timeout: 5), "Accept marks the Task done")
    }

    /// Redo needs a reply, and sends it.
    @MainActor
    func testRedoNeedsATextAndSendsIt() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        XCTAssertFalse(answer(app, "redo").isEnabled, "no text yet")
        tapWhenReady(app.textFields["item-reply"])
        app.textFields["item-reply"].typeText("Keep the old redirect for one week.")
        XCTAssertTrue(answer(app, "redo").isHittable, "the answers stay above the keyboard")
        XCTAssertTrue(answer(app, "redo").isEnabled)
        tapWhenReady(answer(app, "redo"))
        XCTAssertTrue(row(app, "i-0000a002").waitForNonExistence(timeout: 5))
    }

    /// Approve answers an ask, and its agent runs again.
    @MainActor
    func testApproveAnswersAnAsk() throws {
        let app = launchOnInbox()
        open(app, "i-0000a001", question: "Allow Bash: npm run deploy")
        XCTAssertTrue(answer(app, "reject").exists)
        tapWhenReady(answer(app, "approve"))
        XCTAssertTrue(row(app, "i-0000a001").waitForNonExistence(timeout: 5))
        XCTAssertEqual(rowIds(app).first, "i-0000a004")
    }

    /// A question shows the prompt's options as buttons.
    @MainActor
    func testAQuestionIsAnsweredByItsOptions() throws {
        let app = launchOnInbox()
        open(app, "i-0000a004", question: "Which test runner should the suite use?")
        XCTAssertEqual(app.buttons["item-option-0"].label, "Vitest")
        tapWhenReady(app.buttons["item-option-1"])
        XCTAssertTrue(row(app, "i-0000a004").waitForNonExistence(timeout: 5))
    }

    /// Approve on a proposal shows the action, target and content before it is sent.
    @MainActor
    func testApprovingAProposalShowsItFirst() throws {
        let app = launchOnInbox()
        let proposal = row(app, "e-000000a2")
        for _ in 0..<3 where !proposal.isHittable { app.collectionViews["inbox-list"].swipeUp() }
        open(app, "e-000000a2", question: "Close the stale issue ENG-12")
        tapWhenReady(answer(app, "approve"))
        XCTAssertTrue(app.staticTexts["approve-action"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["approve-target"].label, "linear:ENG-12")
        tapWhenReady(app.buttons["approve-confirm"])
        XCTAssertTrue(proposal.waitForNonExistence(timeout: 5))
    }

    /// A swipe asks for Later, and the item leaves the list.
    @MainActor
    func testLaterBySwipe() throws {
        let app = launchOnInbox()
        let failure = row(app, "i-0000a003")
        for _ in 0..<3 where !failure.isHittable { app.collectionViews["inbox-list"].swipeUp() }
        XCTAssertTrue(failure.isHittable)
        // A finger's drag: `swipeLeft()` is too short for a row beside the item on a tablet.
        failure.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: failure.coordinate(withNormalizedOffset: CGVector(dx: 0.05, dy: 0.5)))
        // A full swipe runs Later at once; a shorter one shows its button.
        if !app.buttons["In 1 hour"].waitForExistence(timeout: 2) { tapWhenReady(app.buttons["Later"]) }
        tapWhenReady(app.buttons["In 1 hour"])
        XCTAssertTrue(failure.waitForNonExistence(timeout: 5))
    }

    /// A doc opens in the Reader for the item's pane; closing it returns to the item.
    @MainActor
    func testEvidenceOpensInTheReaderAndReturns() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        tapWhenReady(app.buttons["item-evidence-doc-docs/design.md"])
        XCTAssertTrue(app.webViews.firstMatch.staticTexts["Design"].waitForExistence(timeout: 10))
        tapWhenReady(app.buttons["Done"].firstMatch)
        let shown = app.staticTexts["item-question"]
        XCTAssertTrue(shown.waitForExistence(timeout: 10))
        XCTAssertEqual(shown.label, "Ready for review: Fix the login redirect")
    }
}
