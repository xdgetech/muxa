import XCTest

/// The overall assistant on a `work` level 10 host (ADR 0049), against the demo's
/// `-demo-plans` fixtures: plan p-0000e001 lists w-0000b002 (pinned), w-0000b001 (due in
/// two days) and the item i-0000a001; w-0000b004 is moved down and due in 5 hours; i-0000f001
/// asks work's assistant about w-0000b002 and i-0000f002 sorts c-0000c001; the Result
/// of q-0000d001 proposes notes. The overall workspace is "assistant". Plan Now stores
/// a new plan about 3 seconds later. A new request is q-00000101.
final class PlanTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launchPlans(_ arguments: [String] = []) -> XCUIApplication {
        launchWorkInbox(["-demo-plans"] + arguments)
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }

    @MainActor
    private func todayRow(_ app: XCUIApplication, _ target: String) -> XCUIElement { app.buttons["today-row-\(target)"] }

    @MainActor
    private func openWork(_ app: XCUIApplication, _ id: String) {
        let row = workRow(app, id)
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["work-title"].waitForExistence(timeout: 5))
    }

    @MainActor
    private func openItem(_ app: XCUIApplication, _ id: String) {
        let row = app.buttons["item-row-\(id)"]
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["item-question"].waitForExistence(timeout: 5))
    }

    /// Today is above Decisions with the plan's rows: why, minutes and a Why disclosure.
    /// The Due soon line names the item moved down. A row opens its Work item, which
    /// shows the plan's reason.
    @MainActor
    func testTodayShowsThePlanAboveDecisions() throws {
        let app = launchPlans()
        let today = inboxSection(app, "today")
        XCTAssertTrue(today.waitForExistence(timeout: 5))
        XCTAssertLessThan(today.frame.minY, inboxSection(app, "decisions").frame.minY)
        let pinned = todayRow(app, "w-0000b002")
        XCTAssertTrue(pinned.exists)
        XCTAssertTrue(pinned.label.contains("Ship passkey sign-in"), pinned.label)
        XCTAssertTrue(pinned.label.contains("Pinned by you."), pinned.label)
        XCTAssertTrue(pinned.label.contains("60–120 min"), pinned.label)
        XCTAssertLessThan(pinned.frame.minY, todayRow(app, "w-0000b001").frame.minY, "the pinned item is first")
        XCTAssertTrue(todayRow(app, "i-0000a001").exists, "a row can be an Inbox item")
        tapWhenReady(app.buttons["today-why-w-0000b002"])
        let basis = element(app, "today-basis-w-0000b002")
        XCTAssertTrue(basis.waitForExistence(timeout: 5))
        XCTAssertTrue(basis.label.contains("Pinned by you"), basis.label)
        let dueSoon = element(app, "today-due-soon")
        XCTAssertTrue(dueSoon.exists)
        XCTAssertTrue(dueSoon.label.contains("File the expense report"), dueSoon.label)
        XCTAssertFalse(todayRow(app, "w-0000b004").exists, "an item moved down is never a row")
        save("today")

        tapWhenReady(todayRow(app, "w-0000b001"))
        let title = app.staticTexts["work-title"]
        XCTAssertTrue(title.waitForExistence(timeout: 5))
        XCTAssertEqual(title.label, "Plan the billing migration")
        XCTAssertTrue(element(app, "work-today-why").label.contains("Due in"), "the plan's reason")
    }

    /// Plan Now makes a new plan: New shows, and the selected Work item and the list's
    /// place stay as they were.
    @MainActor
    func testPlanNowShowsANewPlanAndKeepsTheSelection() throws {
        let app = launchPlans()
        // Today and the first decision are at the list's top: no scrolling moves them.
        tapWhenReady(todayRow(app, "w-0000b001"))
        let selected = app.staticTexts["work-title"]
        XCTAssertTrue(selected.waitForExistence(timeout: 5))
        showInboxList(app)
        // A phone pushed the detail; back on the list, only the list's place is kept.
        let keepsDetail = selected.exists
        let decision = app.buttons["item-row-i-0000a001"]
        XCTAssertTrue(decision.waitForExistence(timeout: 5))
        let today = inboxSection(app, "today")
        XCTAssertFalse(today.label.contains("New"), today.label)

        let planNow = app.buttons["today-plan-now"]
        XCTAssertTrue(planNow.waitForExistence(timeout: 5))
        let placeBefore = decision.frame.minY
        tapWhenReady(planNow)
        XCTAssertTrue(element(app, "today-planning").waitForExistence(timeout: 3), "the plan is being made")
        let fresh = expectation(for: NSPredicate(format: "label CONTAINS 'New'"), evaluatedWith: today)
        wait(for: [fresh], timeout: 15)
        XCTAssertFalse(element(app, "today-planning").exists)
        XCTAssertEqual(decision.frame.minY, placeBefore, accuracy: 1, "the list keeps its place")
        if keepsDetail {
            XCTAssertTrue(selected.exists)
            XCTAssertEqual(selected.label, "Plan the billing migration", "the selection is unchanged")
        }
        // Opening a row clears New.
        tapWhenReady(todayRow(app, "w-0000b002"))
        XCTAssertTrue(app.staticTexts["work-title"].waitForExistence(timeout: 5))
        showInboxList(app)
        let seen = expectation(for: NSPredicate(format: "NOT (label CONTAINS 'New')"), evaluatedWith: today)
        wait(for: [seen], timeout: 5)
    }

    /// Approve on a delegate proposal asks work's assistant once: the Work item has one request.
    @MainActor
    func testApprovingADelegateMakesOneRequest() throws {
        let app = launchPlans()
        openItem(app, "i-0000f001")
        XCTAssertTrue(element(app, "item-delegate-workspace").label.contains("work's assistant"))
        XCTAssertTrue(element(app, "item-delegate-purpose").label.contains("conditional UI"))
        tapWhenReady(app.buttons["item-answer-approve"])
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f001"].waitForNonExistence(timeout: 5), "the item is answered")
        openWork(app, "w-0000b002")
        XCTAssertTrue(element(app, "work-request-q-00000101").waitForExistence(timeout: 5))
        let requests = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'work-request-q-'"))
        XCTAssertEqual(requests.count, 1, "one request")
    }

    /// Reject answers a sort proposal and sorts nothing: the Capture stays in Unsorted.
    @MainActor
    func testRejectingASortKeepsTheCapture() throws {
        let app = launchPlans()
        openItem(app, "i-0000f002")
        XCTAssertEqual(element(app, "item-sort-action").label.contains("Make a Work item"), true)
        tapWhenReady(app.buttons["item-answer-reject"])
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f002"].waitForNonExistence(timeout: 5))
        XCTAssertTrue(waitForSidebarRow(app, captureRow(app, "c-0000c001")), "still unsorted")
    }

    /// Pin lists a Work item before the unpinned ones; a due date shows on the item.
    @MainActor
    func testPinMovesAnItemFirstAndADueDateIsSet() throws {
        let app = launchPlans()
        openWork(app, "w-0000b003")
        let pin = app.buttons["work-pin"]
        scrollWorkDetail(app, to: pin)
        tapWhenReady(pin)
        XCTAssertTrue(element(app, "work-pinned").waitForExistence(timeout: 5))
        let setDue = app.buttons["work-set-due"]
        scrollWorkDetail(app, to: setDue)
        tapWhenReady(setDue)
        XCTAssertTrue(element(app, "work-due").waitForExistence(timeout: 5))
        showInboxList(app)
        let domain = workRow(app, "w-0000b003")
        XCTAssertTrue(waitForSidebarRow(app, domain))
        XCTAssertTrue(domain.label.contains("pinned"), domain.label)
        XCTAssertTrue(domain.label.contains("due"), domain.label)
        XCTAssertLessThan(domain.frame.minY, workRow(app, "w-0000b001").frame.minY, "pinned items come first")
    }

    /// A Result's proposed notes: Update Notes writes them once.
    @MainActor
    func testUpdateNotes() throws {
        let app = launchPlans()
        openWork(app, "w-0000b001")
        let update = app.buttons["work-update-notes"]
        scrollWorkDetail(app, to: update)
        XCTAssertTrue(element(app, "work-notes-text").label.contains("Finance picks the switch month."))
        tapWhenReady(update)
        XCTAssertTrue(element(app, "work-notes-applied").waitForExistence(timeout: 5))
        XCTAssertFalse(update.exists)
    }

    /// Notes changed on the host since the run read them: Update Notes says to ask again.
    @MainActor
    func testUpdateNotesConflict() throws {
        let app = launchPlans(["-demo-notes-changed"])
        openWork(app, "w-0000b001")
        let update = app.buttons["work-update-notes"]
        scrollWorkDetail(app, to: update)
        tapWhenReady(update)
        let error = element(app, "work-notes-error")
        XCTAssertTrue(error.waitForExistence(timeout: 5))
        XCTAssertTrue(error.label.contains("changed on the host"), error.label)
    }

    /// The overall assistant's workspace takes no Captures and no Tasks: neither picker offers it.
    @MainActor
    func testTheOverallWorkspaceIsNotOffered() throws {
        let app = launchPlans()
        tapWhenReady(app.buttons["sidebar-capture"])
        let field = element(app, "capture-text")
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        if !waitForFocus(field) { field.tap() }
        field.typeText("Book the venue")
        tapWhenReady(element(app, "capture-workspace"))
        XCTAssertTrue(app.buttons["work"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["assistant"].exists)
        tapWhenReady(app.buttons["work"].firstMatch)

        tapWhenReady(app.buttons["capture-start-task"])
        let workspace = element(app, "record-workspace")
        XCTAssertTrue(workspace.waitForExistence(timeout: 5))
        tapWhenReady(workspace)
        XCTAssertTrue(app.buttons["personal"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["assistant"].exists)
    }

    /// A level 9 host keeps the Inbox as it was: no Today, no pins, no notes.
    @MainActor
    func testALevel9HostShowsNoPlan() throws {
        let app = launchPlans(["-work9"])
        XCTAssertTrue(inboxSection(app, "decisions").exists)
        XCTAssertFalse(inboxSection(app, "today").exists)
        XCTAssertFalse(app.buttons["today-plan-now"].exists)
        XCTAssertFalse(app.buttons["item-row-i-0000f001"].exists, "no proposals")
        openWork(app, "w-0000b001")
        scrollWorkDetail(app, to: app.buttons["work-complete"])
        XCTAssertFalse(app.buttons["work-pin"].exists)
        XCTAssertFalse(app.buttons["work-update-notes"].exists)
    }
}
