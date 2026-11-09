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
    private func launchOnInbox(_ more: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "inbox"] + more
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
        let summary = app.descendants(matching: .any)["item-summary"].firstMatch
        XCTAssertTrue(summary.waitForExistence(timeout: 5))
        let shown = summary.staticTexts.allElementsBoundByIndex.map(\.label)
        XCTAssertTrue(shown.contains("Login redirect"), "the heading: \(shown)")
        XCTAssertTrue(shown.contains { $0.contains("The tests pass.") }, "a list item: \(shown)")
        XCTAssertTrue(summary.links["PR #42"].exists, "the web link is a link")
        XCTAssertFalse(shown.contains { $0.hasPrefix("#") || $0.hasPrefix("- ") || $0.contains("**") },
                       "the summary is Markdown: \(shown)")
        XCTAssertTrue(app.buttons["item-evidence-changes"].label.contains("3 files"))
        XCTAssertTrue(app.buttons["item-evidence-doc-docs/design.md"].label.contains("design.md"))
        XCTAssertTrue(app.buttons["item-evidence-image-build/screenshots/en/login.png"].label.contains("login.png"))
        XCTAssertTrue(app.links["item-evidence-link"].exists || app.buttons["item-evidence-link"].exists)
        XCTAssertEqual(app.buttons["item-send"].label, "Send")
        XCTAssertEqual(answer(app, "keep-open").label, "Done", "✓ closes only the item")
        XCTAssertTrue(answer(app, "later").exists, "Later is in the top bar")
        XCTAssertFalse(answer(app, "accept").exists, "Complete Task is in the … menu")
        save("inbox-item-review")
        tapWhenReady(app.buttons["item-more"])
        XCTAssertEqual(answer(app, "accept").label, "Complete Task")
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
        XCTAssertFalse(app.buttons["item-send"].isEnabled, "no text yet")
        tapWhenReady(app.textFields["item-reply"])
        app.textFields["item-reply"].typeText("Also update the README.")
        XCTAssertTrue(app.buttons["item-send"].isHittable, "the reply box stays above the keyboard")
        XCTAssertTrue(app.buttons["item-send"].isEnabled)
        tapWhenReady(app.buttons["item-send"])
        XCTAssertTrue(row(app, "i-0000a002").waitForNonExistence(timeout: 5))
    }

    /// Deny & Send denies the menu and sends the text to the agent, whose chat shows it.
    @MainActor
    func testDenyAndSendAnswersAnAsk() throws {
        let app = launchOnInbox()
        open(app, "i-0000a001", question: "Allow Bash: npm run deploy")
        XCTAssertEqual(answer(app, "approve").label, "Allow")
        XCTAssertEqual(answer(app, "reject").label, "Deny")
        XCTAssertEqual(app.buttons["item-send"].label, "Deny & Send")
        XCTAssertFalse(app.buttons["item-send"].isEnabled, "no text yet")
        tapWhenReady(app.textFields["item-reply"])
        app.textFields["item-reply"].typeText("Deploy to staging first.")
        tapWhenReady(app.buttons["item-send"])
        XCTAssertTrue(row(app, "i-0000a001").waitForNonExistence(timeout: 5))
        showMode(app, 1)
        // The row opens the session in Chat, its first view (ADR 0044).
        tapWhenReady(app.buttons["task-t-auth01"])
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

    /// A swipe is Later: the item moves to the bottom of the list at once and stays
    /// in it (ADR 0040).
    @MainActor
    func testLaterBySwipeMovesTheItemDown() throws {
        let app = launchOnInbox()
        XCTAssertTrue(row(app, "i-0000a001").waitForExistence(timeout: 10))
        let first = row(app, "i-0000a001")
        let id = first.identifier
        XCTAssertNotEqual(lastRowId(app), id)
        // A finger's drag: `swipeLeft()` is too short for a sidebar row.
        first.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: first.coordinate(withNormalizedOffset: CGVector(dx: 0.05, dy: 0.5)))
        // A full swipe runs Later at once; a shorter one shows its button. The item
        // screen has a Later of its own, for the next item.
        let button = app.collectionViews["sidebar-list"].buttons["Later"]
        if button.waitForExistence(timeout: 2), button.isHittable { button.tap() }
        let moved = expectation(for: NSPredicate { _, _ in self.lastRowId(app) == id }, evaluatedWith: nil)
        wait(for: [moved], timeout: 5)
        XCTAssertTrue(app.buttons[id].exists, "Later hides nothing")
    }

    /// The lowest item row on screen. The query's order is not the list's.
    private func lastRowId(_ app: XCUIApplication) -> String? {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'item-row-'")).allElementsBoundByIndex
            .max { $0.frame.minY < $1.frame.minY }?.identifier
    }

    /// A doc in the attachment list opens in the Reader for the item's pane; closing it
    /// returns to the item, with no sheet to open again.
    @MainActor
    func testEvidenceOpensInTheReaderAndReturns() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        tapWhenReady(app.buttons["item-evidence-doc-docs/design.md"])
        XCTAssertTrue(app.webViews.firstMatch.staticTexts["Design"].waitForExistence(timeout: 10))
        tapWhenReady(app.buttons["reader-done"].firstMatch)
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
        tapWhenReady(app.buttons["reader-done"].firstMatch)
        waitForQuestion(app, "Ready for review: Fix the login redirect")
    }

    /// Open Session in the … menu shows the item's session, and its row stays
    /// highlighted. On a phone, Back returns to the item (ADR 0035 §3).
    @MainActor
    func testOpenSessionShowsItAndTheRowStaysHighlighted() throws {
        let app = launchOnInbox()
        open(app, "i-0000a001", question: "Allow Bash: npm run deploy")
        tapWhenReady(app.buttons["item-more"])
        tapWhenReady(app.buttons["item-open-session-menu"])
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.segmentedControls["view-mode"].buttons["Chat"].isSelected, "its Chat, where the agent has one")
        save("inbox-item-session")
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
        tapWhenReady(app.buttons["item-open-session"])
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.segmentedControls["view-mode"].buttons["Chat"].isSelected)
    }

    /// Ask Agent sends a briefing with a text to the workspace's Ask session, opens
    /// that session, and leaves the item in the list (ADR 0041).
    @MainActor
    func testAskAgentOpensTheWorkspacesAskSessionAndKeepsTheItem() throws {
        let app = launchOnInbox()
        open(app, "e-000000a1", question: "Release moved to Thursday")
        XCTAssertEqual(app.buttons["item-send"].label, "Ask Agent")
        XCTAssertFalse(app.buttons["item-send"].isEnabled, "no text yet")
        tapWhenReady(app.textFields["item-reply"])
        app.textFields["item-reply"].typeText("Put this in Notion.")
        tapWhenReady(app.buttons["item-send"])
        // A phone pushes the pane under the session's name; a tablet shows its view switch.
        let pane = isPhone ? app.navigationBars["ask-work"] : app.segmentedControls["view-mode"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10), "the Ask session's pane")
        if !isPhone {
            // The pane collapsed the sidebar; Inbox mode shows the list again.
            app.typeKey("t", modifierFlags: .command)
            XCTAssertTrue(row(app, "e-000000a1").waitForExistence(timeout: 5), "the item stays open")
        }
    }

    /// An item with a session opens it from its source line; one without has none.
    @MainActor
    func testOnlyAnItemWithASessionOffersItsSession() throws {
        let app = launchOnInbox()
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        XCTAssertTrue(app.buttons["item-open-session"].waitForExistence(timeout: 5))
        if isPhone { tapWhenReady(app.navigationBars.buttons["BackButton"].firstMatch) }
        open(app, "i-0000a004", question: "Which test runner should the suite use?")
        XCTAssertFalse(app.buttons["item-open-session"].exists, "no session runs it")
    }

    /// A swipe right is ✓: the item closes at once (ADR 0040 §6).
    @MainActor
    func testDoneBySwipeRightClosesTheItem() throws {
        let app = launchOnInbox()
        let briefing = row(app, "e-000000a1")
        reveal(app, briefing)
        briefing.coordinate(withNormalizedOffset: CGVector(dx: 0.05, dy: 0.5))
            .press(forDuration: 0.05, thenDragTo: briefing.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)))
        let button = app.collectionViews["sidebar-list"].buttons["Done"]
        if button.waitForExistence(timeout: 2), button.isHittable { button.tap() }
        XCTAssertTrue(briefing.waitForNonExistence(timeout: 5))
    }

    /// Protect in the … menu shows a lock on the row; the item stays open and keeps
    /// its answers. Unprotect takes the lock away (ADR 0046).
    @MainActor
    func testProtectShowsTheLock() throws {
        let app = launchOnInbox()
        XCTAssertFalse(row(app, "i-0000a002").label.contains("Protected"))
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        tapWhenReady(app.buttons["item-more"])
        tapWhenReady(app.buttons["item-protect"])
        XCTAssertTrue(answer(app, "keep-open").waitForExistence(timeout: 5), "the item stays, with ✓")
        tapWhenReady(app.buttons["item-more"])
        XCTAssertTrue(app.buttons["item-unprotect"].waitForExistence(timeout: 5))
        dismissPopover(app)
        if isPhone { tapWhenReady(app.navigationBars.buttons["BackButton"].firstMatch) }
        let locked = NSPredicate(format: "label CONTAINS 'Protected'")
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: locked, object: row(app, "i-0000a002"))],
                                      timeout: 5), .completed, row(app, "i-0000a002").label)
        save("inbox-protected")
    }

    /// Delete Old asks with the count, then removes the old items but keeps a protected
    /// one. With nothing to delete it only says so (ADR 0046). `-old-items` makes the
    /// failure and the last proposal four days old.
    @MainActor
    func testDeleteOldKeepsProtectedItems() throws {
        let app = launchOnInbox(["-old-items"])
        open(app, "i-0000a003", question: "The agent stopped with an error")
        tapWhenReady(app.buttons["item-more"])
        tapWhenReady(app.buttons["item-protect"])
        if isPhone {
            tapWhenReady(app.navigationBars.buttons["BackButton"].firstMatch)
        }
        let old = row(app, "e-000000a3")
        reveal(app, old)
        XCTAssertTrue(old.waitForExistence(timeout: 5))
        XCTAssertTrue(row(app, "i-0000a003").label.contains("Protected"), row(app, "i-0000a003").label)

        deleteOld(app, days: 3)
        let ask = app.alerts.firstMatch
        XCTAssertTrue(ask.waitForExistence(timeout: 5))
        XCTAssertTrue(ask.staticTexts["Delete 1 item older than 3 days? Protected items stay."].exists, ask.debugDescription)
        tapWhenReady(ask.buttons["Delete"])
        XCTAssertTrue(old.waitForNonExistence(timeout: 5), "the old proposal is gone")
        reveal(app, row(app, "i-0000a003"))
        XCTAssertTrue(row(app, "i-0000a003").exists, "the protected item stays")
        XCTAssertTrue(row(app, "i-0000a002").exists, "a new item stays")

        deleteOld(app, days: 5)
        XCTAssertTrue(ask.waitForExistence(timeout: 5))
        XCTAssertTrue(ask.staticTexts["No unprotected items are older than 5 days."].exists, ask.debugDescription)
        XCTAssertFalse(ask.buttons["Delete"].exists)
        tapWhenReady(ask.buttons["OK"])
    }

    /// A host below `work:8` offers neither Protect nor Delete Old.
    @MainActor
    func testAnOlderHostOffersNoProtectOrDeleteOld() throws {
        let app = launchOnInbox(["-work7"])
        XCTAssertFalse(app.buttons["inbox-more"].exists, "no Delete Old")
        open(app, "i-0000a002", question: "Ready for review: Fix the login redirect")
        tapWhenReady(app.buttons["item-more"])
        XCTAssertTrue(answer(app, "accept").waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["item-protect"].exists, "no Protect")
    }

    /// The list's toolbar menu: Delete Old, then the age.
    @MainActor
    private func deleteOld(_ app: XCUIApplication, days: Int) {
        tapWhenReady(app.buttons["inbox-more"])
        tapWhenReady(app.buttons["Delete Old"])
        tapWhenReady(app.buttons[days == 1 ? "Older than 1 day" : "Older than \(days) days"])
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
