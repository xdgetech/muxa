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

    /// At connect, Inbox mode stays on its list (ADR 0035 §6): no pane opens, on a
    /// phone either. It replaces "a phone opens on the Inbox", which the modes decide now.
    @MainActor
    func testConnectStaysOnTheListInInboxMode() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "inbox"]
        app.launch()
        XCTAssertTrue(app.buttons["item-row-i-0000a001"].waitForExistence(timeout: 10))
        sleep(1)
        XCTAssertFalse(app.descendants(matching: .any)["pane"].firstMatch.exists)
        XCTAssertFalse(app.descendants(matching: .any)["composer"].firstMatch.exists)
    }

    /// Tasks mode stays on its list at connect too.
    @MainActor
    func testConnectStaysOnTheListInTasksMode() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "tasks"]
        app.launch()
        XCTAssertTrue(app.buttons["task-t-auth01"].waitForExistence(timeout: 10))
        sleep(1)
        XCTAssertFalse(app.descendants(matching: .any)["pane"].firstMatch.exists)
    }

    /// Sessions mode opens the default pane at connect, as before (ADR 0012): pushed on a phone.
    @MainActor
    func testSessionsModeOpensThePaneAtConnect() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "sessions"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.navigationBars["feature-auth"].exists)
    }

    /// A host below `work:3` has no Inbox (ADR 0033 §5): the Inbox says to update the
    /// bridge, and Tasks still lists its Tasks. It replaces the local Snooze test,
    /// because the older Inbox list is gone.
    @MainActor
    func testAnOlderHostIsToldToUpdateTheBridge() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "inbox", "-work2"]
        app.launch()
        XCTAssertTrue(app.staticTexts["Update muxa-bridge"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["item-row-i-0000a001"].exists, "no items on this host")
        tapWhenReady(app.buttons["sidebar-inbox-tasks"])
        XCTAssertTrue(header(app, "Needs attention").waitForExistence(timeout: 5))
    }

    /// Opens the demo in Tasks mode: the sidebar lists the Tasks.
    @MainActor
    private func launchOnTasks() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "tasks"]
        app.launch()
        XCTAssertTrue(app.buttons["task-t-auth01"].waitForExistence(timeout: 10))
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
        let list = app.collectionViews["sidebar-list"]
        // Routines sit above the groups, so the last group can start below the fold.
        for title in ["Needs attention", "Ready for review", "Running", "No agent"] {
            if !header(app, title).waitForExistence(timeout: 2) { list.swipeUp() }
            XCTAssertTrue(header(app, title).waitForExistence(timeout: 5), title)
        }
        list.swipeDown()
        XCTAssertTrue(app.buttons["task-t-auth01"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Fix the login redirect"].exists)
        save("tasks-inbox")
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

    /// A task's Terminal shows its session's agent pane, and its row stays highlighted.
    @MainActor
    func testATaskOpensItsSessionsPane() throws {
        let app = launchOnTasks()
        tapWhenReady(app.buttons["task-t-login1"])
        XCTAssertTrue(app.staticTexts["After sign-in, return users to the page they came from."].waitForExistence(timeout: 5))
        save("task-detail")
        tapWhenReady(app.buttons["task-open-terminal"])
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, containing: "pane %5 100x30"))
        if isPhone {
            // Back returns to the Task, then to the list.
            tapWhenReady(app.navigationBars.buttons["BackButton"].firstMatch)
            XCTAssertTrue(app.buttons["task-open-terminal"].waitForExistence(timeout: 5))
        } else {
            showSidebar(app)
            XCTAssertTrue(app.buttons["task-t-login1"].waitForExistence(timeout: 5))
            XCTAssertTrue(app.buttons["task-t-login1"].isSelected, "the pane's Task stays highlighted")
        }
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

    /// Opens the New Task sheet from the sidebar's `+` and fills it.
    @MainActor
    private func createTask(
        _ app: XCUIApplication, title: String, goal: String, branch: String? = nil, withLoop: Bool = false
    ) {
        tapWhenReady(app.buttons["sidebar-new-task"])
        fillTask(app, title: title, goal: goal, branch: branch, withLoop: withLoop)
    }

    /// Fills the open New Task sheet and starts the task; with a branch, in the chosen
    /// repo, else the first known one; with a loop, with the workspace's first check.
    @MainActor
    private func fillTask(
        _ app: XCUIApplication, title: String, goal: String, branch: String? = nil, withLoop: Bool = false
    ) {
        let titleField = app.textFields["record-title"]
        XCTAssertTrue(titleField.waitForExistence(timeout: 5))
        tapWhenReady(titleField)
        titleField.typeText(title)
        let goalField = app.textViews["record-goal"].exists ? app.textViews["record-goal"] : app.textFields["record-goal"]
        tapWhenReady(goalField)
        goalField.typeText(goal)
        if let branch {
            let toggle = app.switches["record-with-repo"]
            XCTAssertTrue(toggle.waitForExistence(timeout: 5))
            if (toggle.value as? String) != "1" { tapWhenReady(toggle) }
            // A tap can land on the label; the switch itself sits at the trailing edge.
            if (toggle.value as? String) != "1" { toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap() }
            let branchField = app.textFields["record-branch"]
            // On a phone the keyboard can cover the new fields.
            reveal(app, branchField, in: "record-form")
            tapWhenReady(branchField)
            branchField.typeText(branch)
        }
        if withLoop {
            let toggle = app.switches["record-with-loop"]
            reveal(app, toggle, in: "record-form")
            tapWhenReady(toggle)
            if (toggle.value as? String) != "1" { toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap() }
            let check = app.buttons["loop-check"]
            reveal(app, check, in: "record-form")
            XCTAssertTrue(waitFor(check, labelOrValue: "make check"))
        }
        save("record-new")
        tapWhenReady(app.buttons["record-start"])
    }

    /// In Tasks mode a new task opens its detail, runs, and its Terminal shows its new session.
    @MainActor
    func testANewTaskStartsAnAgentInItsOwnSession() throws {
        let app = launchOnTasks()
        createTask(app, title: "Write the changelog", goal: "List the user-visible changes.")
        XCTAssertTrue(app.navigationBars["Write the changelog"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["List the user-visible changes."].exists)
        let agent = app.descendants(matching: .any)["task-agent-state"].firstMatch
        reveal(app, agent)
        XCTAssertTrue(waitFor(agent, labelOrValue: "running"))
        save("record-created")
        reveal(app, app.buttons["task-open-terminal"], up: true)
        tapWhenReady(app.buttons["task-open-terminal"])
        XCTAssertTrue(app.navigationBars.descendants(matching: .any)["write-the-changelog"].firstMatch
            .waitForExistence(timeout: 10))
    }

    /// Cmd+N opens the record-based sheet in the shown session's repo and the workspace
    /// that lists it. In Sessions mode its session opens, and the new task shows in Tasks.
    @MainActor
    func testCommandNCreatesATaskInTheShownRepo() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "sessions"]
        app.launch()
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))

        composer.typeKey("n", modifierFlags: .command)
        XCTAssertTrue(app.textFields["record-title"].waitForExistence(timeout: 5))
        XCTAssertTrue(waitFor(app.descendants(matching: .any)["record-workspace"].firstMatch, labelOrValue: "work"))
        XCTAssertEqual(app.switches["record-with-repo"].value as? String, "1", "the repo fields are shown")
        XCTAssertTrue(waitFor(app.descendants(matching: .any)["record-repo-choice"].firstMatch, labelOrValue: "app"))
        save("record-new-from-repo")
        fillTask(app, title: "Add search", goal: "Search the docs.", branch: "feat/search")

        XCTAssertTrue(app.navigationBars.descendants(matching: .any)["add-search"].firstMatch
            .waitForExistence(timeout: 10))
        XCTAssertTrue(app.textFields["record-title"].waitForNonExistence(timeout: 5), "the sheet closes")
        showMode(app, 1)
        XCTAssertTrue(app.buttons["task-t-new001"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Add search"].exists)
    }

    /// The sidebar's `+` is New Task, and its modes are on the first screen (ADR 0035 §5).
    @MainActor
    func testTheSidebarOffersTheModesAndNewTask() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "sessions"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        showSidebar(app)
        let modes = app.segmentedControls["sidebar-mode"]
        XCTAssertTrue(modes.waitForExistence(timeout: 5))
        XCTAssertTrue(modes.isHittable)
        XCTAssertEqual(modes.buttons.count, 3)
        save("sidebar-tasks")
        tapWhenReady(app.buttons["sidebar-new-task"])
        XCTAssertTrue(app.textFields["record-title"].waitForExistence(timeout: 5))
        XCTAssertTrue(waitFor(app.descendants(matching: .any)["record-workspace"].firstMatch, labelOrValue: "work"))
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

    /// Scrolls the task detail until `element` can be tapped; a phone shows less of it.
    @MainActor
    private func reveal(_ app: XCUIApplication, _ element: XCUIElement, up: Bool = false, in id: String = "task-detail") {
        let list = app.collectionViews[id]
        let window = app.windows.firstMatch
        for _ in 0..<4 where !(element.exists && element.isHittable) {
            if up { list.swipeDown() } else { list.swipeUp() }
            // While the phone keyboard is up, `swipeUp()` on the New Task form does not
            // scroll it, although the form avoids the keyboard; a finger's drag does.
            if !up, !(element.exists && element.isHittable) {
                window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
                    .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)))
            }
        }
    }

    /// Restarting an idle agent adds a run and leaves a HANDOFF.md to read (ADR 0018).
    @MainActor
    func testRestartingAnIdleAgentAddsARun() throws {
        let app = launchOnTasks()
        tapWhenReady(app.buttons["task-t-login1"])
        XCTAssertFalse(app.buttons["task-open-handoff"].exists, "no restart yet, so no handoff")
        let restart = app.buttons["task-restart"]
        XCTAssertTrue(restart.waitForExistence(timeout: 5))
        reveal(app, restart)
        tapWhenReady(restart)
        let alert = app.alerts["Restart the agent?"]
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        XCTAssertTrue(alert.staticTexts.containing(NSPredicate(format: "label CONTAINS 'reads TASK.md and HANDOFF.md'"))
            .firstMatch.exists)
        XCTAssertFalse(alert.buttons["Stop and restart"].exists, "an idle agent needs no force")
        save("restart-confirm")
        alert.buttons["Restart"].firstMatch.tap()

        let run = app.staticTexts["Run 2 · claude-code"]
        reveal(app, run)
        XCTAssertTrue(run.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Run 1 · claude-code"].exists)
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH 'Restarted · '")).firstMatch.exists)
        save("restart-new-run")

        let handoff = app.buttons["task-open-handoff"]
        // The list shows only the rows on screen, and the Runs pushed the button up.
        reveal(app, handoff, up: true)
        XCTAssertTrue(handoff.waitForExistence(timeout: 5))
        tapWhenReady(handoff)
        XCTAssertTrue(app.webViews.firstMatch.staticTexts["Handoff"].waitForExistence(timeout: 10))
    }

    /// A running agent is stopped only through an explicit "Stop and restart".
    @MainActor
    func testRestartingARunningAgentAsksForForce() throws {
        let app = launchOnTasks()
        tapWhenReady(app.buttons["task-t-docs01"])
        let restart = app.buttons["task-restart"]
        XCTAssertTrue(restart.waitForExistence(timeout: 5))
        reveal(app, restart)
        tapWhenReady(restart)
        let alert = app.alerts["Restart the agent?"]
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        XCTAssertFalse(alert.buttons["Restart"].exists, "a plain restart would be refused")
        save("restart-force")
        alert.buttons["Stop and restart"].firstMatch.tap()
        let run = app.staticTexts["Run 2 · claude-code"]
        reveal(app, run)
        XCTAssertTrue(run.waitForExistence(timeout: 5))
    }

    /// Marking a task done moves it to Closed.
    @MainActor
    func testMarkingATaskDoneClosesIt() throws {
        let app = launchOnTasks()
        tapWhenReady(app.buttons["task-t-login1"])
        // The Loop section can push the status below the fold.
        reveal(app, app.buttons["task-status"])
        tapWhenReady(app.buttons["task-status"])
        tapWhenReady(app.buttons["Done"])
        // The picker shows the saved status once the host has it.
        let saved = NSPredicate(format: "value CONTAINS 'Done' OR label CONTAINS 'Done'")
        XCTAssertEqual(XCTWaiter.wait(for: [XCTNSPredicateExpectation(predicate: saved, object: app.buttons["task-status"])],
                                      timeout: 5), .completed)
        if isPhone { app.navigationBars.buttons["BackButton"].firstMatch.tap() }
        // Running sits just below where Ready for review was.
        XCTAssertTrue(header(app, "Running").waitForExistence(timeout: 5))
        XCTAssertFalse(header(app, "Ready for review").exists, "its only task left the group")
        reveal(app, header(app, "Closed"), in: "sidebar-list")
        XCTAssertTrue(header(app, "Closed").waitForExistence(timeout: 5))
        save("record-done")
    }

    /// Scrolls Tasks to a task, then opens it: on a phone the lower groups are off screen.
    @MainActor
    private func openTask(_ app: XCUIApplication, _ id: String) {
        let row = app.buttons["task-\(id)"]
        XCTAssertTrue(app.collectionViews["sidebar-list"].waitForExistence(timeout: 5))
        reveal(app, row, in: "sidebar-list")
        tapWhenReady(row)
    }

    @MainActor
    private func waitFor(_ element: XCUIElement, labelOrValue text: String) -> Bool {
        let predicate = NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", text, text)
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [exp], timeout: 5) == .completed
    }

    /// Start a stopped loop with the workspace's second listed check, then stop it (ADR 0019).
    @MainActor
    func testALoopStartsWithAListedCheckAndStops() throws {
        let app = launchOnTasks()
        openTask(app, "t-deps01")
        let state = app.staticTexts["loop-state"]
        XCTAssertTrue(waitFor(state, labelOrValue: "Stopped"))
        XCTAssertTrue(waitFor(app.staticTexts["loop-reason"], labelOrValue: "Stopped by the user."))
        let start = app.buttons["loop-start"]
        reveal(app, start)
        tapWhenReady(start)
        let check = app.buttons["loop-check"]
        XCTAssertTrue(waitFor(check, labelOrValue: "make check"), "the last check comes first")
        tapWhenReady(check)
        tapWhenReady(app.buttons["cargo test --workspace"])
        XCTAssertTrue(waitFor(check, labelOrValue: "cargo test --workspace"))
        save("loop-start")
        tapWhenReady(app.buttons["loop-start-confirm"])
        XCTAssertTrue(waitFor(state, labelOrValue: "Armed"))
        XCTAssertTrue(waitFor(app.staticTexts["loop-check-command"], labelOrValue: "cargo test --workspace"))
        save("loop-armed")
        let stop = app.buttons["loop-stop"]
        reveal(app, stop)
        tapWhenReady(stop)
        XCTAssertTrue(waitFor(state, labelOrValue: "Stopped"))
        XCTAssertTrue(app.buttons["loop-start"].waitForExistence(timeout: 5))
    }

    /// A loop paused for the user, or stopped at its limit, puts the task in Needs attention.
    @MainActor
    func testAPausedLoopNeedsAttention() throws {
        let app = launchOnTasks()
        let badge = app.buttons["task-t-auth01"].staticTexts["loop-badge-t-auth01"]
        XCTAssertTrue(badge.waitForExistence(timeout: 5))
        XCTAssertEqual(badge.label, "Loop paused")
        // No agent runs this one: only its loop puts it there.
        let lint = app.buttons["task-t-lint01"]
        XCTAssertTrue(lint.waitForExistence(timeout: 5))
        XCTAssertEqual(app.buttons["task-t-lint01"].staticTexts["loop-badge-t-lint01"].label, "Loop stopped: limit")
        XCTAssertLessThan(header(app, "Needs attention").frame.minY, lint.frame.minY)
        XCTAssertLessThan(lint.frame.minY, header(app, "Ready for review").frame.minY)
        save("loop-inbox")

        openTask(app, "t-auth01")
        XCTAssertTrue(waitFor(app.staticTexts["loop-reason"], labelOrValue: "The loop resumes after the agent finishes its turn."))
        let stop = app.buttons["loop-stop"]
        reveal(app, stop)
        XCTAssertTrue(stop.exists)
        XCTAssertFalse(app.buttons["loop-start"].exists, "an active loop is stopped, not started")
        save("loop-paused")
        if isPhone { app.navigationBars.buttons["BackButton"].firstMatch.tap() }
        openTask(app, "t-lint01")
        XCTAssertTrue(waitFor(app.staticTexts["loop-reason"], labelOrValue: "Stopped: the check failed 3 times."))
        XCTAssertTrue(waitFor(app.staticTexts["loop-last-check"], labelOrValue: "Attempt 3 · exit 1 · 14 s"))
    }

    /// Open check log opens the Reader at the task's CHECK.md.
    @MainActor
    func testTheCheckLogOpensInTheReader() throws {
        let app = launchOnTasks()
        openTask(app, "t-auth01")
        let open = app.buttons["loop-open-log"]
        reveal(app, open)
        tapWhenReady(open)
        let web = app.webViews.firstMatch
        XCTAssertTrue(web.staticTexts["Check"].waitForExistence(timeout: 10))
        save("loop-check-log")
    }

    /// A workspace with listed checks offers its first one; the new task's loop is armed.
    @MainActor
    func testANewTaskRunsACheckAfterEachTurn() throws {
        let app = launchOnTasks()
        createTask(app, title: "Lint the site", goal: "Fix every lint warning.", withLoop: true)
        XCTAssertTrue(app.navigationBars["Lint the site"].waitForExistence(timeout: 10))
        let state = app.staticTexts["loop-state"]
        reveal(app, state)
        XCTAssertTrue(waitFor(state, labelOrValue: "Armed"))
        XCTAssertTrue(waitFor(app.staticTexts["loop-check-command"], labelOrValue: "make check"))
        save("loop-new-task")
    }
}
