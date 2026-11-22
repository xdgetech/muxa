import XCTest

/// The app against its in-app v2 demo server (`-demo`).
final class NextTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    func testHomeShowsTheInboxAndWhoNeedsYou() throws {
        let app = launch()
        for id in ["d1", "d2", "d3", "d4", "d5", "d6"] {
            XCTAssertTrue(app.descendants(matching: .any)["item.\(id)"].exists, id)
        }
        XCTAssertTrue(app.buttons["needsyou.t2"].exists)
    }

    @MainActor
    func testACaptureShowsItsFieldsAndGoesOnTop() throws {
        let app = launch()
        let field = app.textFields["capture.field"]
        field.tap()
        field.typeText("reply Sam about the launch #personal due today")
        XCTAssertTrue(app.buttons["#Personal, remove"].waitForExistence(timeout: 5))
        app.buttons["capture.add"].tap()
        let top = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'item.'")).firstMatch
        XCTAssertTrue(waitFor(top, labelContaining: "Sam about the launch"))
        XCTAssertTrue(top.label.contains("Reply"))
    }

    @MainActor
    func testApproveClosesTheReviewsTask() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d1"].tap()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["detail.title"].label, "Review fix-login")
        app.buttons["detail.main"].tap()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
        // The demo closes the Task's agent, t1.
        backToList(app)
        app.buttons["mode.focus"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["terminal.t3"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["terminal.t1"].exists)
    }

    @MainActor
    func testRequestChangesTypesTheCommentIntoTheAgent() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d1"].tap()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        app.typeKey("r", modifierFlags: .command)
        let comment = app.textFields["detail.comment"]
        XCTAssertTrue(comment.waitForExistence(timeout: 5))
        comment.tap()
        comment.typeText("Use a 30 s margin")
        app.buttons["detail.send"].tap()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
        backToList(app)
        app.buttons["mode.focus"].tap()
        let agent = app.descendants(matching: .any)["terminal.t1"]
        XCTAssertTrue(agent.waitForExistence(timeout: 5))
        agent.tap()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Use a 30 s margin"))
    }

    @MainActor
    func testRetryRunsTheFailedRoutineAgain() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d5"].tap()
        let retry = app.buttons["detail.retry"]
        XCTAssertTrue(retry.waitForExistence(timeout: 5))
        retry.tap()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d5"]))
        backToList(app)
        app.segmentedControls["home.part"].buttons["Routines"].tap()
        let row = app.descendants(matching: .any)["routine.nightly-backup"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        XCTAssertTrue(app.staticTexts["Running since"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testATaskStartsFromTheSheetWithItsLoop() throws {
        let app = launch()
        app.buttons["home.newtask"].tap()
        let title = app.textFields["task.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        title.tap()
        title.typeText("Write the session tests")
        let loop = app.switches["task.loop"]
        XCTAssertTrue(loop.waitForExistence(timeout: 5))
        turnOn(loop)
        XCTAssertTrue(app.descendants(matching: .any)["task.check"].waitForExistence(timeout: 5))
        app.buttons["task.start"].tap()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Starting the agent for Write the session tests"))
        XCTAssertTrue(app.buttons["Loop 0/3"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testAReaderLinkOpensTheLinkedDocument() throws {
        let app = launch()
        app.buttons["needsyou.t2"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.buttons["focus.reader"].tap()
        let plan = app.descendants(matching: .any)["reader.file.PLAN.md"]
        XCTAssertTrue(plan.waitForExistence(timeout: 10))
        plan.tap()
        let link = app.webViews["reader"].links.firstMatch
        XCTAssertTrue(link.waitForExistence(timeout: 10))
        link.tap()
        XCTAssertTrue(waitFor(app.staticTexts["reader.title"], labelContaining: "TASK.md"))
        XCTAssertTrue(app.webViews["reader"].staticTexts["fix-login"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testARoutineRunsNow() throws {
        let app = launch()
        app.segmentedControls["home.part"].buttons["Routines"].tap()
        let row = app.descendants(matching: .any)["routine.release-notes"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        let run = app.buttons["routine.run"]
        XCTAssertTrue(run.waitForExistence(timeout: 5))
        run.tap()
        // The detail (on a phone it covers the list) shows the run started.
        let started = NSPredicate(format: "isEnabled == false")
        XCTAssertEqual(XCTWaiter().wait(for: [expectation(for: started, evaluatedWith: run)], timeout: 10), .completed)
        XCTAssertTrue(app.staticTexts["Running since"].exists)
    }

    @MainActor
    func testNeedsYouOpensTheWaitingAgentInFocus() throws {
        let app = launch()
        app.buttons["needsyou.t2"].tap()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Allow edits to README.md"))
    }

    @MainActor
    func testTypingInFocusReachesTheTerminal() throws {
        let app = launch()
        app.buttons["mode.focus"].tap()
        let shell = app.descendants(matching: .any)["terminal.t3"]
        XCTAssertTrue(shell.waitForExistence(timeout: 5))
        shell.tap()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "$"))
        pane.tap()
        pane.typeText("echo hi\n")
        XCTAssertTrue(waitFor(pane, valueContaining: "echo hi"))
        // The Inbox panel opens beside the terminal.
        app.buttons["focus.inbox"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testTheReaderShowsATasksPlanAndChanges() throws {
        let app = launch()
        app.buttons["needsyou.t2"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.buttons["focus.reader"].tap()
        let plan = app.descendants(matching: .any)["reader.file.PLAN.md"]
        XCTAssertTrue(plan.waitForExistence(timeout: 10))
        plan.tap()
        XCTAssertTrue(app.webViews["reader"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.webViews["reader"].staticTexts["Plan"].waitForExistence(timeout: 10))
        app.buttons["reader.back"].tap()
        app.segmentedControls["reader.kind"].buttons["Changes"].tap()
        let change = app.descendants(matching: .any)["reader.file.src/auth/session.ts"]
        XCTAssertTrue(change.waitForExistence(timeout: 10))
        change.tap()
        XCTAssertTrue(app.webViews["reader"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testATasksLoopStartsAndStops() throws {
        let app = launch()
        app.buttons["needsyou.t2"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.buttons["focus.loop"].tap()
        let start = app.buttons["loop.start"]
        XCTAssertTrue(start.waitForExistence(timeout: 5))
        start.tap()
        XCTAssertTrue(waitForGone(start))
        XCTAssertTrue(app.buttons["Loop 0/3"].waitForExistence(timeout: 5))
        app.buttons["focus.loop"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["loop.now"].firstMatch.waitForExistence(timeout: 5))
        app.buttons["loop.stop"].tap()
        XCTAssertTrue(waitForGone(app.buttons["loop.stop"]))
        let stopped = app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS 'Stopped'")).firstMatch
        XCTAssertTrue(stopped.waitForExistence(timeout: 5))
    }

    @MainActor
    func testChatShowsTheConversationAndAnswersThePrompt() throws {
        let app = launch()
        app.buttons["needsyou.t2"].tap()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        app.buttons["focus.chat"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["chat.prompt"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Fix the README links."].exists)
        app.buttons["chat.option.Allow"].tap()
        // The demo's program echoes what the option sends.
        XCTAssertTrue(waitFor(pane, valueContaining: "(y/n) y"))
    }

    @MainActor
    func testGoToTaskOpensItInFocus() throws {
        let app = launch()
        app.typeKey("p", modifierFlags: .command)
        let field = app.textFields["goto.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("docs\n")
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Allow edits to README.md"))
    }

    @MainActor
    func testDisconnectReturnsToTheHostsAndAHostCanBeAdded() throws {
        let app = launch()
        app.buttons["home.host"].tap()
        app.buttons["Disconnect"].tap()
        let add = app.buttons["next.addhost"]
        XCTAssertTrue(add.waitForExistence(timeout: 5))
        add.tap()
        let address = app.textFields["host-address"]
        XCTAssertTrue(address.waitForExistence(timeout: 5))
        app.textFields["host-name"].tap()
        app.textFields["host-name"].typeText("Nowhere")
        address.tap()
        address.typeText("127.0.0.1")
        app.textFields["host-user"].tap()
        app.textFields["host-user"].typeText("me")
        // Port 22 on the simulator's Mac may answer; a closed port fails at once.
        app.buttons["Connect"].tap()
        XCTAssertTrue(app.buttons["Nowhere"].waitForExistence(timeout: 5))
        let link = app.staticTexts["next.link"]
        XCTAssertTrue(XCTWaiter().wait(for: [expectation(
            for: NSPredicate(format: "NOT (label IN %@)", ["Connecting…", "Not connected"]), evaluatedWith: link)],
            timeout: 20) == .completed)
    }

    /// On a phone the detail covers Home's list: go back to it.
    private func backToList(_ app: XCUIApplication) {
        let back = app.navigationBars.buttons["Home"]
        if !app.buttons["mode.focus"].isHittable, back.exists { back.tap() }
        XCTAssertTrue(app.buttons["mode.focus"].waitForExistence(timeout: 5))
    }

    /// A Form's switch takes taps on its knob, at the trailing edge.
    private func turnOn(_ s: XCUIElement) {
        if s.value as? String != "1" { s.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap() }
        XCTAssertTrue(XCTWaiter().wait(for: [expectation(for: NSPredicate(format: "value == '1'"), evaluatedWith: s)],
                                       timeout: 5) == .completed)
    }

    private func waitFor(_ e: XCUIElement, valueContaining text: String) -> Bool {
        let p = NSPredicate(format: "value CONTAINS %@", text)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }

    private func waitFor(_ e: XCUIElement, labelContaining text: String) -> Bool {
        let p = NSPredicate(format: "label CONTAINS %@", text)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }

    private func waitForGone(_ e: XCUIElement) -> Bool {
        let p = NSPredicate(format: "exists == false")
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }
}
