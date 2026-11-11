import XCTest

/// Today rows that say what to do (ADR 0059) on a `work` level 17 host, against the demo's
/// `-demo-plans` fixtures (see `PlanTests`): w-0000b002 (pinned) and w-0000b001 are Work
/// rows, i-0000a001 is the Task t-auth01's question. The secretary is the overall workspace
/// "assistant", with 2 of 3 runs left today.
final class TodayActionsTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func todayRow(_ app: XCUIApplication, _ target: String) -> XCUIElement { app.buttons["today-row-\(target)"] }

    /// The secretary's Ask session opens, as after Talk to Assistant.
    @MainActor
    private func opensTheSecretarysChat(_ app: XCUIApplication) {
        let pane = isPhone ? app.navigationBars["ask-assistant"] : app.segmentedControls["view-mode"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10), "the secretary's Ask session")
    }

    @MainActor
    private func send(_ app: XCUIApplication, _ text: String) {
        let field = app.alerts.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText(text)
        tapWhenReady(app.alerts.buttons["Send"].firstMatch)
    }

    /// Each row leads with its verb, says Do, and has its main button and a Send that names
    /// who gets the text. Re-rank says the runs left today.
    @MainActor
    func testTodayRowsNameWhatToDo() throws {
        let app = launchWorkInbox(["-demo-plans"])
        let pinned = todayRow(app, "w-0000b002")
        XCTAssertTrue(pinned.waitForExistence(timeout: 5))
        XCTAssertTrue(pinned.label.contains("Start · Ship passkey sign-in"), pinned.label)
        XCTAssertTrue(pinned.label.contains("Do: Pick up where the last session stopped."), pinned.label)
        XCTAssertEqual(app.buttons["today-primary-w-0000b002"].label, "Open Work Item")
        XCTAssertEqual(app.buttons["today-send-w-0000b002"].label,
                       "Send to the secretary (assistant), with this Work item…")

        let question = todayRow(app, "i-0000a001")
        XCTAssertTrue(question.label.contains("Allow or deny · "), question.label)
        XCTAssertEqual(app.buttons["today-primary-i-0000a001"].label, "Open Session")

        let rerank = app.buttons["today-plan-now"]
        XCTAssertTrue(rerank.waitForExistence(timeout: 5))
        XCTAssertTrue(rerank.label.contains("Re-rank"), rerank.label)
        XCTAssertTrue(rerank.label.contains("2 of 3 runs left today"), rerank.label)
        save("today-actions")

        tapWhenReady(rerank)
        let fewer = expectation(for: NSPredicate(format: "label CONTAINS '1 of 3 runs left today'"), evaluatedWith: rerank)
        wait(for: [fewer], timeout: 15)
    }

    /// The question's main button opens the Task's session; the row itself opens the item.
    @MainActor
    func testTheMainButtonOfAQuestionOpensItsSession() throws {
        let app = launchWorkInbox(["-demo-plans"])
        tapWhenReady(app.buttons["today-primary-i-0000a001"])
        let pane = isPhone ? app.navigationBars["feature-auth"] : app.segmentedControls["view-mode"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10), "the Task's session")
    }

    /// Send on a Work row goes to the secretary with the Work item, and its Chat opens.
    @MainActor
    func testAWorkRowSendsToTheSecretary() throws {
        let app = launchWorkInbox(["-demo-plans"])
        tapWhenReady(app.buttons["today-send-w-0000b002"])
        send(app, "Ask legal first.")
        opensTheSecretarysChat(app)
    }

    /// Work detail sends to the secretary too.
    @MainActor
    func testWorkDetailSendsToTheSecretary() throws {
        let app = launchWorkInbox(["-demo-plans"])
        let row = workRow(app, "w-0000b001")
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["work-title"].waitForExistence(timeout: 5))
        let button = app.buttons["work-send-secretary"]
        scrollWorkDetail(app, to: button)
        tapWhenReady(button)
        send(app, "What should I do first?")
        opensTheSecretarysChat(app)
    }

    /// A level 16 host keeps the rows as they were: no verb, no buttons, Plan Now.
    @MainActor
    func testALevel16HostKeepsPlanNow() throws {
        let app = launchWorkInbox(["-demo-plans", "-work16"])
        let pinned = todayRow(app, "w-0000b002")
        XCTAssertTrue(pinned.waitForExistence(timeout: 5))
        XCTAssertFalse(pinned.label.contains("Start · "), pinned.label)
        XCTAssertTrue(pinned.label.contains("Next: "), pinned.label)
        XCTAssertFalse(app.buttons["today-send-w-0000b002"].exists)
        XCTAssertFalse(app.buttons["today-primary-w-0000b002"].exists)
        let planNow = app.buttons["today-plan-now"]
        XCTAssertTrue(planNow.label.contains("Plan Now"), planNow.label)
        XCTAssertFalse(planNow.label.contains("runs left"), planNow.label)
    }
}
