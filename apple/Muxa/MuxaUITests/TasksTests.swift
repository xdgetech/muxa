import XCTest

/// The Tasks sheet against the demo bridge's task records (ADR 0015).
final class TasksTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    /// Opens the demo, then Tasks from the sidebar.
    @MainActor
    private func launchOnTasks() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        showSidebar(app)
        tapWhenReady(app.buttons["open-tasks"])
        XCTAssertTrue(app.navigationBars["Tasks"].waitForExistence(timeout: 5))
        return app
    }

    @MainActor
    private func header(_ app: XCUIApplication, _ title: String) -> XCUIElement {
        app.descendants(matching: .any)["inbox-\(title)"].firstMatch
    }

    /// Every Inbox group shows its demo task; a failed launch shows its note.
    @MainActor
    func testTheInboxGroupsTasksByWhatTheyNeed() throws {
        let app = launchOnTasks()
        for title in ["Needs attention", "Ready for review", "Running", "No agent"] {
            XCTAssertTrue(header(app, title).waitForExistence(timeout: 5), title)
        }
        XCTAssertTrue(app.buttons["task-t-auth01"].exists)
        XCTAssertTrue(app.staticTexts["Fix the login redirect"].exists)
        save("tasks-inbox")
        let list = app.collectionViews.firstMatch
        if !header(app, "Closed").exists { list.swipeUp() }
        XCTAssertTrue(header(app, "Closed").waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Write the release notes"].exists)
        if !app.buttons["task-t-price1"].isHittable { list.swipeDown() }

        tapWhenReady(app.buttons["task-t-price1"])
        let note = app.descendants(matching: .any)["task-note"].firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 5))
        XCTAssertTrue(note.label.contains("did not start"), note.label)
        XCTAssertFalse(app.buttons["task-open-terminal"].exists, "no session to open")
        save("task-detail-note")
    }

    /// A task's Terminal closes the sheet and shows its session's agent pane.
    @MainActor
    func testATaskOpensItsSessionsPane() throws {
        let app = launchOnTasks()
        tapWhenReady(app.buttons["task-t-login1"])
        XCTAssertTrue(app.staticTexts["After sign-in, return users to the page they came from."].waitForExistence(timeout: 5))
        save("task-detail")
        tapWhenReady(app.buttons["task-open-terminal"])
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        // The sheet closes before the pane is pushed.
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, containing: "pane %5 100x30"))
        XCTAssertTrue(app.navigationBars["Tasks"].waitForNonExistence(timeout: 5))
    }

    /// Read TASK.md opens the Reader at the task's goal.
    @MainActor
    func testATaskOpensItsTaskFileInTheReader() throws {
        let app = launchOnTasks()
        tapWhenReady(app.buttons["task-t-docs01"])
        tapWhenReady(app.buttons["task-open-file"])
        let web = app.webViews.firstMatch
        XCTAssertTrue(web.staticTexts["Document the new settings page."].waitForExistence(timeout: 10))
        save("task-file")
    }

    /// Fills the New Task sheet; with a branch, in the first known repo.
    @MainActor
    private func createTask(_ app: XCUIApplication, title: String, goal: String, branch: String? = nil) {
        tapWhenReady(app.buttons["record-new"])
        let titleField = app.textFields["record-title"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        tapWhenReady(titleField)
        titleField.typeText(title)
        let goalField = app.textViews["record-goal"].exists ? app.textViews["record-goal"] : app.textFields["record-goal"]
        tapWhenReady(goalField)
        goalField.typeText(goal)
        if let branch {
            let toggle = app.switches["record-with-repo"]
            tapWhenReady(toggle)
            // A tap can land on the label; the switch itself sits at the trailing edge.
            if (toggle.value as? String) != "1" { toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap() }
            let branchField = app.textFields["record-branch"]
            // On a phone the keyboard can cover the new fields.
            if branchField.exists, !branchField.isHittable { app.collectionViews.firstMatch.swipeUp() }
            tapWhenReady(branchField)
            branchField.typeText(branch)
        }
        save("record-new")
        tapWhenReady(app.buttons["record-start"])
    }

    /// A new task opens its detail, runs, and its Terminal shows its new session.
    @MainActor
    func testANewTaskStartsAnAgentInItsOwnSession() throws {
        let app = launchOnTasks()
        createTask(app, title: "Write the changelog", goal: "List the user-visible changes.")
        XCTAssertTrue(app.navigationBars["Write the changelog"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["List the user-visible changes."].exists)
        XCTAssertTrue(app.staticTexts["running"].waitForExistence(timeout: 5))
        save("record-created")
        tapWhenReady(app.buttons["task-open-terminal"])
        XCTAssertTrue(app.navigationBars.descendants(matching: .any)["write-the-changelog"].firstMatch
            .waitForExistence(timeout: 10))
    }

    /// A launch that fails keeps the task, and its detail says why.
    @MainActor
    func testAFailedLaunchKeepsTheTaskWithItsNote() throws {
        let app = launchOnTasks()
        createTask(app, title: "Reuse a branch", goal: "Continue the old work.", branch: "taken")
        let note = app.descendants(matching: .any)["task-note"].firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 10))
        XCTAssertTrue(note.label.contains("already exists"), note.label)
        save("record-launch-failed")
    }

    /// Marking a task done moves it to Closed.
    @MainActor
    func testMarkingATaskDoneClosesIt() throws {
        let app = launchOnTasks()
        tapWhenReady(app.buttons["task-t-login1"])
        tapWhenReady(app.buttons["task-status"])
        tapWhenReady(app.buttons["Done"])
        // The picker shows the saved status once the host has it.
        let saved = NSPredicate(format: "value CONTAINS 'Done' OR label CONTAINS 'Done'")
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: saved, object: app.buttons["task-status"])],
                                      timeout: 5), .completed)
        app.navigationBars.buttons.firstMatch.tap()
        XCTAssertTrue(header(app, "Closed").waitForExistence(timeout: 5))
        XCTAssertFalse(header(app, "Ready for review").exists, "its only task left the group")
        save("record-done")
    }
}
