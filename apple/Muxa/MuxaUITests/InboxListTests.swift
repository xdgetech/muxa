import XCTest

/// The Inbox list against the demo's Task places, stored entries and Routines
/// (ADR 0025 "List first", ADR 0026, ADR 0027 §4). It runs as a host at `work`
/// level 2 (`-work2`): a host with Inbox items gets the list of `InboxItemTests`,
/// and an older one keeps this list unchanged.
final class InboxListTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    /// Opens the demo on the Inbox list: a phone opens there by itself.
    @MainActor
    private func launchOnInbox() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-work2"]
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
    private func row(_ app: XCUIApplication, _ id: String) -> XCUIElement { app.buttons["inbox-row-\(id)"] }

    /// The rows' ids, top to bottom, as far as the list has them on screen.
    @MainActor
    private func rowIds(_ app: XCUIApplication) -> [String] {
        app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'inbox-row-'")).allElementsBoundByIndex
            .sorted { $0.frame.minY < $1.frame.minY }
            .map { String($0.identifier.dropFirst("inbox-row-".count)) }
    }

    /// Scrolls until `element` can be tapped: the entries sit below the Tasks.
    @MainActor
    private func reveal(_ app: XCUIApplication, _ element: XCUIElement) {
        for _ in 0..<4 where !(element.exists && element.isHittable) {
            app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.7))
                .press(forDuration: 0.05, thenDragTo: app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35)))
        }
    }

    @MainActor
    private func act(_ app: XCUIApplication, _ id: String, _ action: String) {
        reveal(app, app.buttons["inbox-actions-\(id)"])
        tapWhenReady(app.buttons["inbox-actions-\(id)"])
        tapWhenReady(app.buttons[action].firstMatch)
    }

    /// Task needs come first, then the waiting Routine run, then briefings and proposals.
    @MainActor
    func testTheListHoldsTasksRoutineRunsAndEntries() throws {
        let app = launchOnInbox()
        XCTAssertTrue(row(app, "t-auth01").waitForExistence(timeout: 5))
        XCTAssertEqual(rowIds(app).first, "t-auth01")
        save("inbox-list")
        let run = row(app, "t-r00002")
        // The list makes rows only as they come on screen.
        reveal(app, run)
        XCTAssertTrue(run.exists)
        XCTAssertTrue(run.label.contains("Routine run waits for your answer"), run.label)
        reveal(app, row(app, "e-000000a1"))
        XCTAssertTrue(row(app, "e-000000a1").exists)
        reveal(app, run)
        tapWhenReady(run)
        XCTAssertTrue(app.navigationBars["linear-triage-t-r00002"].waitForExistence(timeout: 10), "Act opens the run's terminal")
    }

    /// Move down puts a Task at the end of the list, below every entry; its status stays.
    @MainActor
    func testMoveDownPutsATaskLast() throws {
        let app = launchOnInbox()
        XCTAssertTrue(row(app, "t-auth01").waitForExistence(timeout: 5))
        act(app, "t-auth01", "Move Down")
        XCTAssertTrue(row(app, "t-lint01").waitForExistence(timeout: 5))
        let moved = row(app, "t-auth01")
        reveal(app, row(app, "e-000000a3"))
        reveal(app, moved)
        XCTAssertTrue(moved.isHittable)
        XCTAssertGreaterThan(moved.frame.minY, row(app, "e-000000a3").frame.minY, "below the last entry")
        XCTAssertEqual(rowIds(app).last, "t-auth01")
        save("inbox-moved-down")
        tapWhenReady(app.segmentedControls["inbox-mode"].buttons["All Tasks"])
        XCTAssertTrue(app.descendants(matching: .any)["inbox-Needs attention"].firstMatch.waitForExistence(timeout: 5),
                      "Move down does not change the group")
    }

    /// Later hides a Task until the chosen time.
    @MainActor
    func testLaterHidesATask() throws {
        let app = launchOnInbox()
        let task = row(app, "t-auth01")
        XCTAssertTrue(task.waitForExistence(timeout: 5))
        act(app, "t-auth01", "Later…")
        tapWhenReady(app.buttons["In 1 hour"])
        XCTAssertTrue(task.waitForNonExistence(timeout: 5))
    }

    /// Approve shows the proposal as the Task gets it, creates exactly one Task and opens it.
    @MainActor
    func testApprovingAProposalCreatesOneTaskAndOpensIt() throws {
        let app = launchOnInbox()
        let title = "Close the stale issue ENG-12"
        XCTAssertTrue(app.descendants(matching: .any)["inbox-list"].firstMatch.waitForExistence(timeout: 5))
        act(app, "e-000000a2", "Approve…")
        XCTAssertTrue(app.staticTexts["approve-action"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["approve-action"].label, "Close the issue")
        XCTAssertEqual(app.staticTexts["approve-target"].label, "linear:ENG-12")
        XCTAssertTrue(app.staticTexts["approve-content"].label.contains("no activity for 30 days"))
        save("inbox-approve")
        tapWhenReady(app.buttons["approve-confirm"])
        // The new Task's detail opens; its goal carries the proposal unchanged.
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'muxa-proposal: e-000000a2'"))
            .firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["task-open-terminal"].exists)
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(row(app, "e-000000a2").waitForNonExistence(timeout: 5))
        tapWhenReady(app.segmentedControls["inbox-mode"].buttons["All Tasks"])
        XCTAssertTrue(app.descendants(matching: .any)["all-tasks"].firstMatch.waitForExistence(timeout: 5))
        let tasks = app.staticTexts.matching(NSPredicate(format: "label == %@", title))
        let list = app.collectionViews["all-tasks"]
        for _ in 0..<3 where tasks.count == 0 { list.swipeUp() }
        XCTAssertEqual(tasks.count, 1)
    }

    /// Dismiss removes a proposal and creates nothing.
    @MainActor
    func testDismissRemovesAProposal() throws {
        let app = launchOnInbox()
        let entry = row(app, "e-000000a3")
        reveal(app, entry)
        XCTAssertTrue(entry.exists)
        act(app, "e-000000a3", "Dismiss")
        XCTAssertTrue(entry.waitForNonExistence(timeout: 5))
    }

    /// A briefing opens with its text and source; Done acknowledges it.
    @MainActor
    func testABriefingIsReadThenDone() throws {
        let app = launchOnInbox()
        reveal(app, row(app, "e-000000a1"))
        tapWhenReady(row(app, "e-000000a1"))
        XCTAssertTrue(app.staticTexts["entry-body"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.links["Release thread"].exists || app.buttons["Release thread"].exists)
        reveal(app, app.buttons["entry-done"])
        tapWhenReady(app.buttons["entry-done"])
        XCTAssertTrue(row(app, "e-000000a1").waitForNonExistence(timeout: 5))
    }

    /// A Routine's detail shows its fields; Run now starts a run, and a disabled one cannot run.
    @MainActor
    func testARoutineShowsItsFieldsAndRunsNow() throws {
        let app = launchOnInbox()
        tapWhenReady(app.segmentedControls["inbox-mode"].buttons["All Tasks"])
        tapWhenReady(app.buttons["open-routines"])
        tapWhenReady(app.buttons["routine-work/slack-digest"])
        let field = { (id: String) in app.descendants(matching: .any)[id].firstMatch }
        XCTAssertTrue(field("routine-trigger").waitForExistence(timeout: 5))
        XCTAssertTrue(field("routine-workspace").label.contains("work"), field("routine-workspace").label)
        XCTAssertTrue(field("routine-trigger").label.contains("08:00 Europe/Berlin"), field("routine-trigger").label)
        XCTAssertTrue(field("routine-enabled").label.contains("Yes"))
        XCTAssertTrue(field("routine-last-success").label.contains("3 new"), field("routine-last-success").label)
        XCTAssertTrue(field("routine-next-due").exists)
        XCTAssertTrue(app.staticTexts["routine-failure-reason"].label.contains("output.json"))
        XCTAssertFalse(field("routine-running").exists)
        save("routine-detail")
        let run = app.buttons["routine-run-now"]
        reveal(app, run)
        tapWhenReady(run)
        let disabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == false"), object: run)
        XCTAssertEqual(XCTWaiter.wait(for: [disabled], timeout: 5), .completed, "one run at a time")
        let window = app.windows.firstMatch
        window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
            .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75)))
        XCTAssertTrue(field("routine-running").waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["routine-open-terminal"].exists)

        app.navigationBars.buttons.firstMatch.tap()
        tapWhenReady(app.buttons["routine-personal/weekly-memory"])
        XCTAssertTrue(field("routine-trigger").waitForExistence(timeout: 5))
        XCTAssertTrue(field("routine-trigger").label.contains("Run now only"))
        reveal(app, app.buttons["routine-run-now"])
        XCTAssertFalse(app.buttons["routine-run-now"].isEnabled, "a disabled Routine never runs")
    }

    /// The running run's Open Terminal shows its session.
    @MainActor
    func testARunningRoutineOpensItsTerminal() throws {
        let app = launchOnInbox()
        tapWhenReady(app.segmentedControls["inbox-mode"].buttons["All Tasks"])
        tapWhenReady(app.buttons["open-routines"])
        tapWhenReady(app.buttons["routine-work/linear-triage"])
        reveal(app, app.buttons["routine-open-terminal"])
        tapWhenReady(app.buttons["routine-open-terminal"])
        XCTAssertTrue(app.navigationBars["linear-triage-t-r00002"].waitForExistence(timeout: 10))
    }
}
