import XCTest

/// The Inbox as a list of questions (ADR 0033) against the demo's items: an ask with
/// a permission, a question, a review with evidence, a failure, a proposal and a briefing.
/// The list is the sidebar's Inbox mode, and the item is the detail (ADR 0035): it
/// was a sheet over the pane before, so every test opens it from the sidebar now.
final class InboxItemTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    /// The demo starts in Inbox mode, and connect stays on the list.
    @MainActor
    private func launchOnInbox() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "inbox"]
        app.launch()
        XCTAssertTrue(row(app, "i-0000a001").waitForExistence(timeout: 10))
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

    /// Opens an item: pushed on a phone, in the detail on a tablet.
    @MainActor
    private func open(_ app: XCUIApplication, _ id: String, question: String) {
        tapWhenReady(row(app, id))
        waitForQuestion(app, question, "\(id) is open")
    }

    @MainActor
    private func waitForQuestion(_ app: XCUIApplication, _ question: String, _ message: String = "",
                                 file: StaticString = #filePath, line: UInt = #line) {
        let shown = app.staticTexts["item-question"]
        let exp = XCTNSPredicateExpectation(predicate: NSPredicate(format: "label == %@", question), object: shown)
        XCTAssertEqual(XCTWaiter.wait(for: [exp], timeout: 5), .completed, message, file: file, line: line)
    }

    /// Scrolls the sidebar until `element` can be tapped.
    @MainActor
    private func reveal(_ app: XCUIApplication, _ element: XCUIElement) {
        for _ in 0..<3 where !element.isHittable { app.collectionViews["sidebar-list"].swipeUp() }
    }

    /// Asks come first, then the newest; each row has the question, its source and its age.
    /// The whole list shows, with no fold; a phone stays on the list at connect.
    @MainActor
    func testTheListShowsItems() throws {
        let app = launchOnInbox()
        XCTAssertEqual(Array(rowIds(app).prefix(3)), ["i-0000a001", "i-0000a004", "i-0000a002"])
        XCTAssertTrue(row(app, "i-0000a001").label.contains("Allow Bash: npm run deploy"), row(app, "i-0000a001").label)
        XCTAssertTrue(row(app, "i-0000a001").label.contains("Add sign-in with passkeys · 1m"), row(app, "i-0000a001").label)
        XCTAssertTrue(row(app, "e-000000a2").exists, "a Routine's proposal is an item too")
        XCTAssertFalse(app.buttons["show-more"].exists, "no fold")
        XCTAssertFalse(app.descendants(matching: .any)["pane"].firstMatch.exists, "connect stays on the list")
        if isPhone {
            XCTAssertFalse(app.staticTexts["item-question"].exists, "a phone opens no item by itself")
        } else {
            // A tablet shows the first item beside the list.
            waitForQuestion(app, "Allow Bash: npm run deploy")
            XCTAssertTrue(row(app, "i-0000a001").isSelected)
        }
        save("inbox-items")
    }

    /// A review shows its summary and evidence; Done removes it, and the Task is in Closed.
    @MainActor
    func testDoneRemovesAReview() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        XCTAssertTrue(app.staticTexts["item-summary"].label.contains("PR #42"))
        XCTAssertFalse(app.staticTexts["item-summary"].label.contains("**"), "the summary is Markdown")
        XCTAssertTrue(app.buttons["item-evidence-changes"].label.contains("3 files"))
        XCTAssertTrue(app.buttons["item-evidence-doc-docs/design.md"].label.contains("design.md"))
        XCTAssertTrue(app.buttons["item-evidence-image-build/screenshots/en/login.png"].label.contains("login.png"))
        XCTAssertTrue(app.links["item-evidence-link"].exists || app.buttons["item-evidence-link"].exists)
        XCTAssertEqual(answer(app, "accept").label, "Done")
        XCTAssertTrue(app.staticTexts["item-result-accept"].label.contains("closes the Task"))
        XCTAssertEqual(answer(app, "reply").label, "Send")
        for action in ["redo", "change-course", "keep-open"] {
            XCTAssertFalse(answer(app, action).exists, "\(action) is under More")
        }
        save("inbox-item-review")
        tapWhenReady(answer(app, "accept"))
        XCTAssertTrue(row(app, "i-0000a002").waitForNonExistence(timeout: 5))
        if !isPhone {
            // A tablet shows the item that took its place.
            waitForQuestion(app, "Release moved to Thursday")
        }
        showMode(app, 1)
        let closed = app.descendants(matching: .any)["inbox-Closed"].firstMatch
        for _ in 0..<3 where !closed.exists { app.collectionViews["sidebar-list"].swipeUp() }
        XCTAssertTrue(closed.waitForExistence(timeout: 5), "Accept marks the Task done")
    }

    /// Send needs a reply, and sends it as it is (ADR 0033 §4a.1).
    @MainActor
    func testSendRepliesToAReview() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        XCTAssertFalse(answer(app, "reply").isEnabled, "no text yet")
        tapWhenReady(app.textFields["item-reply"])
        app.textFields["item-reply"].typeText("Also update the README.")
        XCTAssertTrue(answer(app, "reply").isHittable, "the answers stay above the keyboard")
        XCTAssertTrue(answer(app, "reply").isEnabled)
        tapWhenReady(answer(app, "reply"))
        XCTAssertTrue(row(app, "i-0000a002").waitForNonExistence(timeout: 5))
    }

    /// Redo is under More, needs a reply, and sends it.
    @MainActor
    func testRedoIsUnderMoreAndSendsTheReply() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        tapWhenReady(app.textFields["item-reply"])
        app.textFields["item-reply"].typeText("Keep the old redirect for one week.")
        tapWhenReady(app.buttons["item-more"])
        tapWhenReady(answer(app, "redo"))
        XCTAssertTrue(row(app, "i-0000a002").waitForNonExistence(timeout: 5))
    }

    /// Deny & Send denies the menu and sends the text to the agent, whose chat shows it.
    @MainActor
    func testDenyAndSendAnswersAnAsk() throws {
        let app = launchOnInbox()
        open(app, "i-0000a001", question: "Allow Bash: npm run deploy")
        XCTAssertEqual(answer(app, "approve").label, "Allow")
        XCTAssertEqual(answer(app, "reject").label, "Deny")
        XCTAssertEqual(answer(app, "reply").label, "Deny & Send")
        XCTAssertFalse(answer(app, "reply").isEnabled, "no text yet")
        tapWhenReady(app.textFields["item-reply"])
        app.textFields["item-reply"].typeText("Deploy to staging first.")
        tapWhenReady(answer(app, "reply"))
        XCTAssertTrue(row(app, "i-0000a001").waitForNonExistence(timeout: 5))
        showMode(app, 1)
        tapWhenReady(app.buttons["task-t-auth01"])
        tapWhenReady(app.buttons["task-open-chat"])
        let sent = app.staticTexts.matching(NSPredicate(format: "label CONTAINS %@", "Deploy to staging first.")).firstMatch
        XCTAssertTrue(sent.waitForExistence(timeout: 10), "the agent got the text")
    }

    /// Allow answers an ask, and its agent runs again. A phone returns to the list.
    @MainActor
    func testAllowAnswersAnAsk() throws {
        let app = launchOnInbox()
        open(app, "i-0000a001", question: "Allow Bash: npm run deploy")
        XCTAssertTrue(answer(app, "reject").exists)
        tapWhenReady(answer(app, "approve"))
        XCTAssertTrue(row(app, "i-0000a001").waitForNonExistence(timeout: 5))
        XCTAssertTrue(row(app, "i-0000a004").waitForExistence(timeout: 5))
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
        reveal(app, proposal)
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
        reveal(app, failure)
        XCTAssertTrue(failure.isHittable)
        // A finger's drag: `swipeLeft()` is too short for a sidebar row.
        failure.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: failure.coordinate(withNormalizedOffset: CGVector(dx: 0.05, dy: 0.5)))
        // A full swipe runs Later at once; a shorter one shows its button.
        if !app.buttons["In 1 hour"].waitForExistence(timeout: 2) { tapWhenReady(app.buttons["Later"]) }
        tapWhenReady(app.buttons["In 1 hour"])
        XCTAssertTrue(failure.waitForNonExistence(timeout: 5))
    }

    /// A doc in the attachment list opens in the Reader for the item's pane; closing it
    /// returns to the item, with no sheet to open again.
    @MainActor
    func testEvidenceOpensInTheReaderAndReturns() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        tapWhenReady(app.buttons["item-evidence-doc-docs/design.md"])
        XCTAssertTrue(app.webViews.firstMatch.staticTexts["Design"].waitForExistence(timeout: 10))
        tapWhenReady(app.buttons["Done"].firstMatch)
        waitForQuestion(app, "Ready for review: Fix the login redirect")
        XCTAssertFalse(app.descendants(matching: .any)["pane"].firstMatch.exists, "the item, not the pane")
    }

    /// A file name in the summary that is an attachment opens it in the Reader too.
    @MainActor
    func testAFileNameInTheSummaryOpensTheReader() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        tapWhenReady(app.links["docs/design.md"])
        XCTAssertTrue(app.webViews.firstMatch.staticTexts["Design"].waitForExistence(timeout: 10))
        tapWhenReady(app.buttons["Done"].firstMatch)
        waitForQuestion(app, "Ready for review: Fix the login redirect")
    }

    /// Terminal shows the item's session in the Agent view, and its row stays
    /// highlighted. On a phone, Back returns to the item (ADR 0035 §3).
    @MainActor
    func testTerminalShowsTheAgentAndTheRowStaysHighlighted() throws {
        let app = launchOnInbox()
        open(app, "i-0000a001", question: "Allow Bash: npm run deploy")
        tapWhenReady(app.buttons["item-open-terminal"])
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        XCTAssertTrue(app.segmentedControls["view-mode"].buttons["Agent"].isSelected, "Terminal opens Agent, not Chat")
        XCTAssertFalse(app.descendants(matching: .any)["composer"].firstMatch.exists)
        save("inbox-item-terminal")
        if isPhone {
            tapWhenReady(app.navigationBars.buttons["BackButton"].firstMatch)
            waitForQuestion(app, "Allow Bash: npm run deploy", "Back returns to the item")
        } else {
            showSidebar(app)
            XCTAssertTrue(row(app, "i-0000a001").waitForExistence(timeout: 5))
            XCTAssertTrue(row(app, "i-0000a001").isSelected, "the pane's item stays highlighted")
            // The row shows its item again.
            tapWhenReady(row(app, "i-0000a001"))
            waitForQuestion(app, "Allow Bash: npm run deploy")
        }
    }

    /// Chat shows the item's session in the Chat view, where an ask is answered.
    @MainActor
    func testChatShowsTheConversation() throws {
        let app = launchOnInbox()
        open(app, "i-0000a001", question: "Allow Bash: npm run deploy")
        tapWhenReady(app.buttons["item-open-chat"])
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.segmentedControls["view-mode"].buttons["Chat"].isSelected)
    }

    /// An item with a session offers Chat and Terminal; one without a session neither.
    @MainActor
    func testOnlyAnItemWithASessionOffersItsSession() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        XCTAssertTrue(app.buttons["item-open-terminal"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["item-open-chat"].exists)
        if isPhone { tapWhenReady(app.navigationBars.buttons["BackButton"].firstMatch) }
        open(app, "i-0000a004", question: "Which test runner should the suite use?")
        XCTAssertFalse(app.buttons["item-open-terminal"].exists, "no session runs it")
    }

    /// From the pane, Cmd+T, Cmd+L and Cmd+Shift+S pick the modes and show the sidebar;
    /// the pane stays the detail, since the sidebar never changes what it shows by itself.
    @MainActor
    func testKeysPickTheModes() throws {
        guard !isPhone else { throw XCTSkip("needs the iPad's hardware keyboard") }
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "sessions"]
        app.launch()
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        // From the agent's terminal, which has the keyboard, as in
        // `DemoTests.testTypingReachesThePaneAndShortcutsSwitch`.
        composer.typeKey("r", modifierFlags: .command)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        XCTAssertTrue(waitForFocus(pane))
        pane.typeKey("t", modifierFlags: .command)
        XCTAssertTrue(row(app, "i-0000a001").waitForExistence(timeout: 5), "Inbox mode")
        XCTAssertTrue(pane.exists, "the pane stays")
        pane.typeKey("l", modifierFlags: .command)
        XCTAssertTrue(app.buttons["open-routines"].waitForExistence(timeout: 5), "Tasks mode")
        pane.typeKey("s", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.staticTexts["Other Sessions"].waitForExistence(timeout: 5), "Sessions mode")
        pane.typeKey(",", modifierFlags: .command)
        XCTAssertTrue(app.buttons["host-Demo"].waitForExistence(timeout: 5), "Settings")
    }

    /// Cmd+O opens the shown item's terminal.
    @MainActor
    func testCommandOOpensTheItemsTerminal() throws {
        guard !isPhone else { throw XCTSkip("needs the iPad's hardware keyboard") }
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        // A key needs a focused view: the reply box.
        tapWhenReady(app.textFields["item-reply"])
        app.textFields["item-reply"].typeKey("o", modifierFlags: .command)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, containing: "pane %5 100x30"))
    }
}
