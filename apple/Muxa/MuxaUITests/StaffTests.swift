import XCTest

/// Staff and hand-offs on a `work` level 12 host (ADR 0051), against the demo's
/// `-demo-staff` fixtures: `-demo-policies` (see `PolicyTests`) with chief-of-staff (the
/// lead), researcher and writer on "work" and one member on "personal". w-0000b005's
/// request q-0000d002 ran chief-of-staff → researcher, both with proposed notes, and
/// waits on i-0000f004 to hand it to the writer: a `result` item, or a `handoff` item with
/// `-work15`. i-0000f007 is q-0000d001's Result, which no member wrote. The delegate
/// i-0000f001 names the researcher and is Today's last row.
final class StaffTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launchStaff(_ arguments: [String] = []) -> XCUIApplication {
        launchWorkInbox(["-demo-staff"] + arguments)
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

    /// Scrolls the detail to `target` and says whether it is there.
    @MainActor
    private func reveal(_ app: XCUIApplication, _ target: XCUIElement) -> Bool {
        scrollWorkDetail(app, to: target)
        return target.exists
    }

    /// Ask offers the staff only for a workspace with more than one member: "Let the lead
    /// decide" first, each member with its description. Asking the researcher runs its step.
    @MainActor
    func testAskOffersTheStaffOnlyWithMoreThanOneMember() throws {
        let app = launchStaff()
        openWork(app, "w-0000b003")
        XCTAssertTrue(reveal(app, app.buttons["work-ask"]))
        XCTAssertFalse(element(app, "work-ask-staff-lead").exists, "personal has one member")

        openWork(app, "w-0000b002")
        let lead = element(app, "work-ask-staff-lead")
        XCTAssertTrue(reveal(app, lead))
        XCTAssertTrue(lead.label.contains("Let chief-of-staff decide"), lead.label)
        XCTAssertTrue(lead.isSelected, "the lead decides unless you pick")
        let researcher = element(app, "work-ask-staff-researcher")
        XCTAssertTrue(reveal(app, researcher))
        XCTAssertTrue(researcher.label.contains("compares options"), "the description is its subtitle: \(researcher.label)")
        save("staff-picker")
        tapWhenReady(researcher)
        XCTAssertTrue(researcher.isSelected)
        let ask = app.buttons["work-ask"]
        XCTAssertTrue(reveal(app, ask))
        ask.tap()
        let step = element(app, "work-step-1")
        XCTAssertTrue(step.waitForExistence(timeout: 5))
        XCTAssertTrue(step.label.contains("researcher"), step.label)
        XCTAssertEqual(element(app, "work-chain").label, "researcher")
        XCTAssertTrue(app.staticTexts["work-result-summary-step-1"].waitForExistence(timeout: 20))
    }

    /// Capture's Ask offers the staff of the picked workspace, and none for a workspace of one.
    @MainActor
    func testCaptureOffersTheStaffOfItsWorkspace() throws {
        let app = launchStaff()
        tapWhenReady(app.buttons["sidebar-capture"])
        let field = element(app, "capture-text")
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        if !waitForFocus(field) { field.tap() }
        field.typeText("Compare two EU tax advisers")
        XCTAssertFalse(element(app, "capture-staff-lead").exists, "no workspace, no staff")
        tapWhenReady(element(app, "capture-workspace"))
        tapWhenReady(app.buttons["personal"].firstMatch)
        XCTAssertFalse(element(app, "capture-staff-lead").waitForExistence(timeout: 2), "personal has one member")
        tapWhenReady(element(app, "capture-workspace"))
        tapWhenReady(app.buttons["work"].firstMatch)
        // Below the scope and purpose: the keyboard and a phone's height hide it.
        let writer = element(app, "capture-staff-writer")
        for _ in 0..<4 where !(writer.exists && writer.isHittable) {
            app.collectionViews["capture-form"].swipeUp()
        }
        tapWhenReady(writer)
        tapWhenReady(app.buttons["capture-ask"])
        XCTAssertTrue(field.waitForNonExistence(timeout: 5), "Ask closes the sheet")
        openWork(app, "w-00000105")
        let step = element(app, "work-step-1")
        XCTAssertTrue(step.waitForExistence(timeout: 5))
        XCTAssertTrue(step.label.contains("writer"), step.label)
    }

    /// Work detail shows the chain, each step with its delivery and Result, and the
    /// waiting hand-off greyed with Approve. Approve starts the writer's step once.
    @MainActor
    func testTheChainShowsEachStepAndApproveStartsTheNextOnce() throws {
        let app = launchStaff()
        openWork(app, "w-0000b005")
        let chain = element(app, "work-chain")
        XCTAssertTrue(chain.waitForExistence(timeout: 5))
        XCTAssertEqual(chain.label, "chief-of-staff → researcher → writer")
        XCTAssertTrue(element(app, "work-step-1").label.contains("chief-of-staff"))
        XCTAssertTrue(element(app, "work-step-delivery-1").label.contains("result in"))
        let second = app.staticTexts["work-result-summary-step-2"]
        XCTAssertTrue(reveal(app, second))
        XCTAssertTrue(second.label.hasPrefix("Provider B"), second.label)
        let next = element(app, "work-step-next")
        XCTAssertTrue(reveal(app, next))
        XCTAssertTrue(next.label.contains("writer"), next.label)
        let approve = app.buttons["work-handoff-approve"]
        XCTAssertTrue(reveal(app, approve))
        XCTAssertTrue(app.buttons["work-handoff-reject"].exists)
        save("staff-chain")

        approve.tap()
        let third = element(app, "work-step-3")
        XCTAssertTrue(third.waitForExistence(timeout: 5))
        XCTAssertTrue(third.label.contains("writer"), third.label)
        XCTAssertTrue(approve.waitForNonExistence(timeout: 5))
        XCTAssertFalse(element(app, "work-step-next").exists, "nothing waits now")
        XCTAssertFalse(element(app, "work-step-4").exists, "one new step")
        // The list makes only the rows on screen: back to the top for the chain.
        for _ in 0..<6 where !chain.exists { app.collectionViews["work-detail"].swipeDown() }
        XCTAssertEqual(chain.label, "chief-of-staff → researcher → writer", "the writer once")
        showInboxList(app)
        XCTAssertFalse(waitForSidebarRow(app, app.buttons["item-row-i-0000f004"]), "the hand-off is answered")
    }

    /// A level 15 host's `handoff` item names both members; Reject ends the chain with the
    /// results it has.
    @MainActor
    func testRejectingAHandoffEndsTheChain() throws {
        let app = launchStaff(["-work15"])
        let row = app.buttons["item-row-i-0000f004"]
        XCTAssertTrue(waitForSidebarRow(app, row))
        XCTAssertTrue(row.label.contains("researcher → writer"), row.label)
        openItem(app, "i-0000f004")
        let question = app.staticTexts["item-question"]
        XCTAssertTrue(question.label.contains("researcher") && question.label.contains("writer"), question.label)
        XCTAssertTrue(element(app, "item-handoff-to").label.contains("writer"))
        XCTAssertTrue(app.buttons["item-answer-approve"].exists)
        XCTAssertTrue(app.buttons["item-answer-later"].exists)
        save("item-handoff")
        tapWhenReady(app.buttons["item-answer-reject"])
        XCTAssertFalse(app.alerts.firstMatch.waitForExistence(timeout: 2), "no reason asked")
        showInboxList(app)
        XCTAssertTrue(row.waitForNonExistence(timeout: 5))

        openWork(app, "w-0000b005")
        let outcome = element(app, "work-handoff-outcome")
        XCTAssertTrue(reveal(app, outcome))
        XCTAssertTrue(outcome.label.contains("Rejected"), outcome.label)
        XCTAssertFalse(app.buttons["work-handoff-approve"].exists)
        XCTAssertFalse(element(app, "work-step-3").exists)
    }

    /// At level 16 the hand-off waits in a `result` item (ADR 0058): the summary, Findings and
    /// Next Steps, Hand off by the next member's name, the reply box and ✓ Done. Hand off
    /// starts the writer's step.
    @MainActor
    func testAResultItemShowsTheResultAndHandsOff() throws {
        let app = launchStaff()
        let row = app.buttons["item-row-i-0000f004"]
        XCTAssertTrue(waitForSidebarRow(app, row))
        XCTAssertTrue(row.label.contains("researcher → writer"), row.label)
        openItem(app, "i-0000f004")
        let question = app.staticTexts["item-question"]
        XCTAssertTrue(question.label.hasPrefix("researcher asks writer to: Draft"), question.label)
        XCTAssertTrue(element(app, "item-summary").exists)
        XCTAssertTrue(element(app, "item-result-finding-0").label.contains("all 27 EU formats"))
        XCTAssertTrue(element(app, "item-result-next-0").label.contains("decision memo"))
        let handOff = app.buttons["item-answer-approve"]
        XCTAssertTrue(handOff.waitForExistence(timeout: 5))
        XCTAssertEqual(handOff.label, "Hand off to writer")
        XCTAssertEqual(app.buttons["item-send"].label, "Send")
        XCTAssertTrue(app.buttons["item-answer-dismiss"].exists, "✓ is Done")
        XCTAssertFalse(app.buttons["item-answer-reject"].exists)
        save("item-result")
        handOff.tap()
        showInboxList(app)
        XCTAssertTrue(row.waitForNonExistence(timeout: 5), "answered")

        openWork(app, "w-0000b005")
        let third = element(app, "work-step-3")
        XCTAssertTrue(reveal(app, third))
        XCTAssertTrue(third.label.contains("writer"), third.label)
    }

    /// Send asks the researcher again as version 2 of the request; the item is answered.
    @MainActor
    func testSendOnAResultAsksTheSameMemberAgain() throws {
        let app = launchStaff()
        openItem(app, "i-0000f004")
        let reply = app.textFields["item-reply"]
        XCTAssertTrue(reply.waitForExistence(timeout: 5))
        XCTAssertEqual(reply.placeholderValue, "Tell researcher what to do next…")
        tapWhenReady(reply)
        reply.typeText("Also compare their prices for 2027.")
        tapWhenReady(app.buttons["item-send"])
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f004"].waitForNonExistence(timeout: 5), "answered")

        openWork(app, "w-0000b005")
        let asked = element(app, "work-request-q-0000d002")
        XCTAssertTrue(asked.waitForExistence(timeout: 5))
        XCTAssertTrue(asked.label.contains("version 2"), asked.label)
        let step = element(app, "work-step-1")
        XCTAssertTrue(reveal(app, step))
        XCTAssertTrue(step.label.contains("researcher"), step.label)
        XCTAssertFalse(element(app, "work-step-next").exists, "the new version waits on no hand-off")
    }

    /// Done closes the item and ends the chain: the hand-off never starts.
    @MainActor
    func testDoneOnAResultEndsTheChain() throws {
        let app = launchStaff()
        openItem(app, "i-0000f004")
        tapWhenReady(app.buttons["item-answer-dismiss"])
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000f004"].waitForNonExistence(timeout: 5))

        openWork(app, "w-0000b005")
        let outcome = element(app, "work-handoff-outcome")
        XCTAssertTrue(reveal(app, outcome))
        XCTAssertTrue(outcome.label.contains("Done"), outcome.label)
        XCTAssertFalse(element(app, "work-step-3").exists)
    }

    /// A Result no member wrote offers no hand-off; Send tells the assistant.
    @MainActor
    func testAResultWithoutAMemberOffersSendAndDone() throws {
        let app = launchStaff()
        openItem(app, "i-0000f007")
        let question = app.staticTexts["item-question"]
        XCTAssertTrue(question.label.hasPrefix("The assistant finished: Two ways fit."), question.label)
        XCTAssertTrue(element(app, "item-result-question-0").label.contains("open invoices"))
        XCTAssertTrue(element(app, "item-result-file-0").label.hasSuffix("plan-mapping.md"))
        XCTAssertFalse(app.buttons["item-answer-approve"].exists)
        XCTAssertTrue(app.buttons["item-answer-dismiss"].exists)
        XCTAssertEqual(app.textFields["item-reply"].placeholderValue, "Tell the assistant what to do next…")
    }

    /// Each step proposes its own notes: updating the researcher's leaves the lead's.
    @MainActor
    func testEachStepUpdatesItsOwnNotes() throws {
        let app = launchStaff()
        openWork(app, "w-0000b005")
        let update = app.buttons["work-update-notes-step-2"]
        XCTAssertTrue(reveal(app, update))
        XCTAssertTrue(element(app, "work-notes-text-step-2").label.contains("EU invoice formats"))
        update.tap()
        XCTAssertTrue(element(app, "work-notes-applied-step-2").waitForExistence(timeout: 5))
        app.collectionViews["work-detail"].swipeDown()
        app.collectionViews["work-detail"].swipeDown()
        XCTAssertTrue(reveal(app, app.buttons["work-update-notes-step-1"]), "the lead's notes are not applied")
    }

    /// Today's delegate row and its item name the staff member the plan picked.
    @MainActor
    func testTheDelegateNamesTheStaffMember() throws {
        let app = launchStaff()
        let row = app.buttons["today-row-i-0000f001"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(row.label.contains("researcher in work"), row.label)
        openItem(app, "i-0000f001")
        XCTAssertTrue(element(app, "item-delegate-workspace").label.contains("researcher in work"))
    }

    /// A level 11 host shows none of it: no picker, no chain, no hand-off item.
    @MainActor
    func testALevel11HostShowsNoStaff() throws {
        let app = launchStaff(["-work11"])
        XCTAssertFalse(app.buttons["item-row-i-0000f004"].exists, "no hand-off items")
        let today = app.buttons["today-row-i-0000f001"]
        XCTAssertTrue(today.waitForExistence(timeout: 5))
        XCTAssertFalse(today.label.contains("researcher"), "no member named: \(today.label)")
        openWork(app, "w-0000b005")
        XCTAssertTrue(app.staticTexts["work-result-summary"].waitForExistence(timeout: 5))
        XCTAssertFalse(element(app, "work-chain").exists)
        XCTAssertTrue(reveal(app, app.buttons["work-ask"]))
        XCTAssertFalse(element(app, "work-ask-staff-lead").exists)
        XCTAssertFalse(app.buttons["work-handoff-approve"].exists)
    }
}
