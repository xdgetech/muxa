import XCTest

/// Proposed Tasks (ADR 0060) on a `work` level 18 host, against the demo's `-demo-proposals`
/// fixtures: `-demo-staff` (see `StaffTests`), where the researcher's Result i-0000f004 also
/// proposes "Add the provider B client" beside its hand-off, and i-0000f007 proposes "Keep
/// the old invoice numbers"; the secretary's i-0000f009 is Today's last row; i-0000f00a is
/// the plan of t-login1, in the session fix-login.
final class ProposalTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launchProposals(_ arguments: [String] = []) -> XCUIApplication {
        // Today's rows push the items, which `launchWorkInbox` waits for, below a small
        // screen: wait for Today.
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "inbox", "-demo-proposals"] + arguments
        app.launch()
        XCTAssertTrue(inboxSection(app, "today").waitForExistence(timeout: 10), "the demo's Inbox")
        return app
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }

    @MainActor
    private func openItem(_ app: XCUIApplication, _ id: String) {
        showInboxList(app)
        let row = app.buttons["item-row-\(id)"]
        XCTAssertTrue(waitForSidebarRow(app, row), "row \(id)")
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["item-question"].waitForExistence(timeout: 5), "item \(id) opened")
    }

    /// A Result that proposes a Task shows the card; Start Task starts it, and the item
    /// stays with Open its Task.
    @MainActor
    func testStartTaskOnAResultStartsItsTask() throws {
        let app = launchProposals()
        openItem(app, "i-0000f007")
        XCTAssertEqual(element(app, "item-task-title").label, "Keep the old invoice numbers")
        let place = element(app, "item-task-where").label
        XCTAssertTrue(place.contains("its own folder"), place)
        XCTAssertTrue(element(app, "item-task-plan-mode").label.contains("plan mode"))
        let start = app.buttons["item-answer-approve"]
        XCTAssertEqual(start.label, "Start Task")
        save("item-proposed-task")
        tapWhenReady(start)
        let open = app.buttons["item-open-task"]
        XCTAssertTrue(open.waitForExistence(timeout: 5), "the item stays, with its Task")
        XCTAssertTrue(app.staticTexts["item-question"].exists)
        tapWhenReady(open)
        XCTAssertTrue(app.staticTexts["item-question"].waitForNonExistence(timeout: 10), "its Task opened")
    }

    /// Beside a hand-off, Start Task is the main button and Approve still hands off.
    @MainActor
    func testStartTaskBesideAHandoff() throws {
        let app = launchProposals()
        openItem(app, "i-0000f004")
        XCTAssertEqual(element(app, "item-task-title").label, "Add the provider B client")
        let place = element(app, "item-task-where").label
        XCTAssertTrue(place.contains("repo app"), place)
        XCTAssertEqual(app.buttons["item-answer-approve"].label, "Hand off to writer")
        let start = app.buttons["item-answer-start-task"]
        XCTAssertEqual(start.label, "Start Task")
        tapWhenReady(start)
        XCTAssertTrue(app.buttons["item-open-task"].waitForExistence(timeout: 5))
    }

    /// The secretary's proposal is Today's Start Task row. Send tells the secretary what to
    /// change and leaves it open; Start Task starts it.
    @MainActor
    func testTheSecretarysProposal() throws {
        let app = launchProposals()
        let row = app.buttons["today-row-i-0000f009"]
        XCTAssertTrue(waitForSidebarRow(app, row), "Today's row")
        XCTAssertTrue(row.label.contains("Start Task · "), row.label)
        XCTAssertEqual(app.buttons["today-send-i-0000f009"].label, "Send to the secretary…")
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["item-question"].waitForExistence(timeout: 5))
        let goal = element(app, "item-task-goal").label
        XCTAssertTrue(goal.contains("no summary"), goal)
        let reply = app.textFields["item-reply"]
        XCTAssertEqual(reply.placeholderValue, "Tell the secretary what to change…")
        tapWhenReady(reply)
        reply.typeText("Use the docs repo instead.")
        tapWhenReady(app.buttons["item-send"])
        XCTAssertTrue(app.buttons["item-answer-approve"].waitForExistence(timeout: 5), "still open")
        tapWhenReady(app.buttons["item-answer-approve"])
        XCTAssertTrue(app.buttons["item-open-task"].waitForExistence(timeout: 5))
    }

    /// A plan item approves the plan or keeps planning.
    @MainActor
    func testAPlanItemApprovesThePlan() throws {
        let app = launchProposals()
        openItem(app, "i-0000f00a")
        XCTAssertEqual(app.buttons["item-answer-approve"].label, "Approve plan")
        XCTAssertEqual(app.buttons["item-answer-reject"].label, "Keep planning")
        XCTAssertEqual(app.textFields["item-reply"].placeholderValue, "Keep planning & send…")
        tapWhenReady(app.buttons["item-answer-approve"])
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f00a"].waitForNonExistence(timeout: 5), "answered")
    }

    /// Where the pane shows no "Yes, manually approve edits", the plan is approved in the
    /// terminal: the alert opens the Task's session.
    @MainActor
    func testAChangedPlanMenuSaysToApproveInTheTerminal() throws {
        let app = launchProposals(["-plan-menu-changed"])
        openItem(app, "i-0000f00a")
        tapWhenReady(app.buttons["item-answer-approve"])
        let open = app.alerts.buttons["Open Session"].firstMatch
        XCTAssertTrue(open.waitForExistence(timeout: 5), "approve in the terminal")
        open.tap()
        let pane = isPhone ? app.navigationBars["fix-login"] : app.segmentedControls["view-mode"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10), "the Task's session")
    }

    /// A level 17 host has no proposals: no card, no Start Task, no secretary's item.
    @MainActor
    func testALevel17HostOffersNoStartTask() throws {
        let app = launchProposals(["-work17"])
        openItem(app, "i-0000f007")
        XCTAssertFalse(element(app, "item-task-title").exists)
        XCTAssertFalse(app.buttons["item-answer-approve"].exists)
        showInboxList(app)
        XCTAssertFalse(app.buttons["item-row-i-0000f009"].exists)
        XCTAssertFalse(app.buttons["today-row-i-0000f009"].exists)
    }
}
