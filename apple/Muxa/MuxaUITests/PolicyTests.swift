import XCTest

/// Policies, corrections and hypotheses on a `work` level 11 host (ADR 0050), against
/// the demo's `-demo-policies` fixtures: `-demo-plans` (see `PlanTests`) and policies
/// y-0000a101 (this week, all), y-0000a102 (work, no end) and the expired y-0000a103;
/// corrections x-0000a103 (Not today, with a reason), x-0000a102 and x-0000a101; the
/// proposed hypothesis h-0000a101 grounded on x-0000a103 and x-0000a102, asked as the
/// item i-0000f003. Today's row w-0000b001 cites y-0000a102, h-0000a101, x-0000a103 and
/// the forgotten x-0000a1ff. A new policy is y-0000a111.
final class PolicyTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launchPolicies(_ arguments: [String] = []) -> XCUIApplication {
        launchWorkInbox(["-demo-policies"] + arguments)
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }

    @MainActor
    private func todayRow(_ app: XCUIApplication, _ target: String) -> XCUIElement { app.buttons["today-row-\(target)"] }

    /// A row's menu: Move Up, Move Down and Not Today.
    @MainActor
    private func rowMenu(_ app: XCUIApplication, _ target: String, _ choice: String) {
        let row = todayRow(app, target)
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.press(forDuration: 1.2)
        tapWhenReady(app.buttons[choice].firstMatch)
    }

    /// Today's Policies button: an item lower in the list may have scrolled it away.
    @MainActor
    private func openPolicies(_ app: XCUIApplication) {
        let button = app.buttons["today-policies"]
        for _ in 0..<5 where !(button.exists && button.isHittable) {
            app.collectionViews["sidebar-list"].swipeDown()
        }
        XCTAssertTrue(waitForSidebarRow(app, button))
        tapWhenReady(button)
        XCTAssertTrue(element(app, "policies-list").waitForExistence(timeout: 5))
    }

    /// Scrolls the Policies list until `element` shows: corrections come last.
    @MainActor
    private func revealInPolicies(_ app: XCUIApplication, _ target: XCUIElement) -> Bool {
        for _ in 0..<5 where !target.waitForExistence(timeout: 1) {
            element(app, "policies-list").swipeUp()
        }
        return target.exists
    }

    @MainActor
    private func openItem(_ app: XCUIApplication, _ id: String) {
        let row = app.buttons["item-row-\(id)"]
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["item-question"].waitForExistence(timeout: 5))
    }

    /// Types an optional reason into the alert and confirms it.
    @MainActor
    private func giveReason(_ app: XCUIApplication, _ identifier: String, _ reason: String) {
        let field = app.alerts.textFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        if !reason.isEmpty {
            if !waitForFocus(field) { field.tap() }
            field.typeText(reason)
        }
        tapWhenReady(app.alerts.buttons[identifier == "reject" ? "Reject" : "Not Today"].firstMatch)
    }

    /// Move Up puts a row first in the user's order; Not today collapses a row to one
    /// line. The Policies screen lists both as corrections, the reason with its row.
    @MainActor
    func testTodayReorderAndNotToday() throws {
        let app = launchPolicies()
        let pinned = todayRow(app, "w-0000b002")
        XCTAssertTrue(pinned.waitForExistence(timeout: 5))
        XCTAssertLessThan(pinned.frame.minY, todayRow(app, "w-0000b001").frame.minY, "the plan's order first")
        rowMenu(app, "w-0000b001", "Move Up")
        let moved = expectation(
            for: NSPredicate { _, _ in
                self.todayRow(app, "w-0000b001").frame.minY < self.todayRow(app, "w-0000b002").frame.minY
            }, evaluatedWith: nil)
        wait(for: [moved], timeout: 5)

        rowMenu(app, "i-0000a001", "Not Today…")
        giveReason(app, "not-today", "Ask Dana tomorrow")
        let dismissed = element(app, "today-dismissed-i-0000a001")
        XCTAssertTrue(dismissed.waitForExistence(timeout: 5))
        XCTAssertFalse(todayRow(app, "i-0000a001").exists, "one line instead of the row")
        save("today-not-today")

        openPolicies(app)
        let list = element(app, "policies-list")
        let reason = list.staticTexts.containing(NSPredicate(format: "label CONTAINS 'Ask Dana tomorrow'")).firstMatch
        XCTAssertTrue(revealInPolicies(app, reason), "the reason is listed")
        let reordered = list.staticTexts.containing(NSPredicate(format: "label CONTAINS 'You reordered Today'")).firstMatch
        XCTAssertTrue(revealInPolicies(app, reordered))
    }

    /// Why shows the policy, the hypothesis and the correction by text, and "forgotten"
    /// for a record the host no longer has. Forgetting the correction also removes the
    /// hypothesis it grounds, and its Inbox item.
    @MainActor
    func testWhyCitesRecordsAndForgetRemovesThem() throws {
        let app = launchPolicies()
        tapWhenReady(app.buttons["today-why-w-0000b001"])
        let basis = element(app, "today-basis-w-0000b001")
        XCTAssertTrue(basis.waitForExistence(timeout: 5))
        XCTAssertTrue(basis.label.contains("Policy: Keep mornings free for deep work."), basis.label)
        XCTAssertTrue(basis.label.contains("Hypothesis: Billing work waits on finance"), basis.label)
        XCTAssertTrue(basis.label.contains("Not today:"), basis.label)
        XCTAssertTrue(basis.label.contains("Correction: forgotten"), basis.label)
        save("today-why-policies")

        let forget = app.buttons["today-forget-x-0000a103"]
        revealInSidebar(app, forget)
        tapWhenReady(forget)
        tapWhenReady(app.buttons["forget-confirm"].firstMatch)
        let gone = expectation(for: NSPredicate(format: "NOT (label CONTAINS 'Billing work waits')"), evaluatedWith: basis)
        wait(for: [gone], timeout: 5)
        XCTAssertFalse(basis.label.contains("Not today:"), basis.label)
        XCTAssertFalse(app.buttons["today-forget-x-0000a103"].exists)
        openPolicies(app)
        XCTAssertTrue(element(app, "policy-y-0000a102").waitForExistence(timeout: 5))
        XCTAssertFalse(element(app, "hypothesis-h-0000a101").exists, "the hypothesis went with its ground")
        XCTAssertFalse(revealInPolicies(app, element(app, "correction-x-0000a103")))
        XCTAssertTrue(element(app, "correction-x-0000a102").exists)
        tapWhenReady(app.buttons["policies-done"])
        XCTAssertFalse(waitForSidebarRow(app, app.buttons["item-row-i-0000f003"]), "its Inbox item went too")
    }

    /// Add a policy for this week, edit its words, then delete it. Expired policies
    /// are listed apart; the overall workspace is no scope.
    @MainActor
    func testAddEditAndDeleteAPolicy() throws {
        let app = launchPolicies()
        openPolicies(app)
        XCTAssertTrue(element(app, "policy-y-0000a101").exists)
        XCTAssertTrue(element(app, "policy-y-0000a103").label.contains("Until"), "the expired one keeps its end")
        tapWhenReady(app.buttons["policy-add"])
        let text = element(app, "policy-text")
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        if !waitForFocus(text) { text.tap() }
        text.typeText("Hiring calls before noon.")
        tapWhenReady(element(app, "policy-scope"))
        XCTAssertTrue(app.buttons["work"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["assistant"].exists, "the overall workspace is no scope")
        tapWhenReady(app.buttons["work"].firstMatch)
        tapWhenReady(element(app, "policy-end"))
        tapWhenReady(app.buttons["This week"].firstMatch)
        tapWhenReady(app.buttons["policy-save"])
        let added = element(app, "policy-y-0000a111")
        XCTAssertTrue(added.waitForExistence(timeout: 5))
        XCTAssertTrue(added.label.contains("Hiring calls before noon."), added.label)
        XCTAssertTrue(added.label.contains("work · Until"), added.label)
        save("policies")

        tapWhenReady(added)
        let edit = element(app, "policy-text")
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        if !waitForFocus(edit) { edit.tap() }
        edit.typeText(" And on Fridays.")
        tapWhenReady(app.buttons["policy-save"])
        let edited = expectation(for: NSPredicate(format: "label CONTAINS 'And on Fridays.'"), evaluatedWith: added)
        wait(for: [edited], timeout: 5)

        tapWhenReady(added)
        tapWhenReady(app.buttons["policy-delete"])
        XCTAssertTrue(added.waitForNonExistence(timeout: 5))
    }

    /// A hypothesis item says what it rests on; Confirm keeps it as a policy.
    @MainActor
    func testConfirmingAHypothesisMakesAPolicy() throws {
        let app = launchPolicies()
        openItem(app, "i-0000f003")
        XCTAssertTrue(element(app, "item-hypothesis-text").label.contains("Billing work waits on finance"))
        XCTAssertTrue(element(app, "item-hypothesis-evidence").label.contains("Based on 2 corrections"))
        save("item-hypothesis")
        tapWhenReady(app.buttons["item-answer-confirm"])
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f003"].waitForNonExistence(timeout: 5), "the item is answered")
        openPolicies(app)
        let policy = element(app, "policies-list").buttons.containing(
            NSPredicate(format: "label CONTAINS 'Billing work waits on finance'")).firstMatch
        XCTAssertTrue(policy.waitForExistence(timeout: 5), "the hypothesis is a policy now")
        XCTAssertFalse(element(app, "hypothesis-h-0000a101").exists)
    }

    /// Reject offers a reason and needs none: the hypothesis shows as rejected, and the
    /// correction keeps the reason.
    @MainActor
    func testRejectingAHypothesisWithAReason() throws {
        let app = launchPolicies()
        openItem(app, "i-0000f003")
        tapWhenReady(app.buttons["item-answer-reject"])
        giveReason(app, "reject", "It is the budget")
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f003"].waitForNonExistence(timeout: 5))
        openPolicies(app)
        let hypothesis = element(app, "hypothesis-h-0000a101")
        XCTAssertTrue(revealInPolicies(app, hypothesis))
        XCTAssertTrue(hypothesis.staticTexts.containing(NSPredicate(format: "label CONTAINS 'Rejected'")).firstMatch.exists)
        let reason = element(app, "policies-list").staticTexts
            .containing(NSPredicate(format: "label CONTAINS 'It is the budget'")).firstMatch
        XCTAssertTrue(revealInPolicies(app, reason))
    }

    /// Settings reaches the same list.
    @MainActor
    func testSettingsOpensPolicies() throws {
        let app = launchPolicies()
        tapWhenReady(app.buttons["open-settings"])
        tapWhenReady(app.buttons["settings-policies"])
        XCTAssertTrue(element(app, "policy-y-0000a101").waitForExistence(timeout: 5))
    }

    /// A level 10 host keeps Today as it was: no Policies, no Not today, no hypotheses,
    /// and Reject asks for no reason.
    @MainActor
    func testALevel10HostHasNoPolicies() throws {
        let app = launchPolicies(["-work10"])
        XCTAssertTrue(todayRow(app, "w-0000b001").waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["today-policies"].exists)
        XCTAssertFalse(app.buttons["item-row-i-0000f003"].exists, "no hypothesis items")
        tapWhenReady(app.buttons["today-why-w-0000b001"])
        XCTAssertFalse(app.buttons["today-forget-x-0000a103"].exists)
        openItem(app, "i-0000f002")
        tapWhenReady(app.buttons["item-answer-reject"])
        XCTAssertFalse(app.alerts.firstMatch.waitForExistence(timeout: 2), "no reason asked")
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f002"].waitForNonExistence(timeout: 5))
    }
}
