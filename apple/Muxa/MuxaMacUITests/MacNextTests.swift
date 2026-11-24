import XCTest

/// The Mac app against its in-app v2 demo server (`-demo`), as the iOS `NextTests`.
final class MacNextTests: XCTestCase {
    override func setUp() {
        super.setUp()
        continueAfterFailure = false
    }

    @MainActor
    func launch() -> XCUIApplication {
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
    func testACaptureGoesOnTop() throws {
        let app = launch()
        // Home's Capture button opens the Quick Capture panel; ⏎ adds and closes it.
        app.buttons["home.capture"].click()
        let field = app.textFields["capture.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("reply Sam about the launch #personal due today")
        XCTAssertTrue(app.buttons["#Personal, remove"].waitForExistence(timeout: 5))
        field.typeText("\r")
        XCTAssertTrue(waitForGone(field))
        let top = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'item.'")).firstMatch
        XCTAssertTrue(waitFor(top, labelContaining: "Sam about the launch"))
    }

    @MainActor
    func testLaterMovesAnItemToTheBottom() throws {
        let app = launch()
        let rows = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'item.'"))
        XCTAssertEqual(rows.firstMatch.identifier, "item.d1")
        app.descendants(matching: .any)["item.d1"].click()
        app.buttons["detail.later"].click()
        let moved = NSPredicate(format: "identifier != 'item.d1'")
        XCTAssertEqual(XCTWaiter().wait(for: [expectation(for: moved, evaluatedWith: rows.firstMatch)], timeout: 10),
                       .completed)
        // Nothing is hidden: the item is the last row.
        app.scrollViews["inbox.list"].scroll(byDeltaX: 0, deltaY: -5000)
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 5))
        XCTAssertEqual(rows.allElementsBoundByIndex.last?.identifier, "item.d1")
    }

    @MainActor
    func testApproveClosesTheReviewsTask() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d1"].click()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        app.typeKey(.return, modifierFlags: .command)
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
        // The demo closes the Task's agent, t1.
        app.typeKey("f", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["terminal.t2"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["terminal.t1"].exists)
    }

    @MainActor
    func testAnItemsNoteShowsAsMarkdown() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d3"].click()
        let note = app.descendants(matching: .any)["detail.note"].firstMatch
        XCTAssertTrue(note.waitForExistence(timeout: 5))
        let shown = note.staticTexts.allElementsBoundByIndex.map { $0.value as? String ?? $0.label }
        XCTAssertTrue(shown.contains { $0.contains("pgvector") }, "a list item: \(shown)")
        XCTAssertFalse(shown.contains { $0.hasPrefix("- ") || $0.contains("**") || $0.contains("`") },
                       "the note is Markdown: \(shown)")
        XCTAssertTrue(note.links["release notes"].exists, "the web link is a link")
    }

    @MainActor
    func testMakeItATaskStartsATaskFromAReadItem() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d4"].click()
        let make = app.buttons["detail.maketask"]
        XCTAssertTrue(make.waitForExistence(timeout: 5))
        make.click()
        let title = app.textFields["task.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.value as? String, "Weekly dependency digest")
        app.buttons["task.start"].click()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Starting the agent for Weekly dependency digest"))
    }

    @MainActor
    func testPaneKeysSplitAShellAndCloseIt() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["pane.1"].waitForExistence(timeout: 5))
        XCTAssertFalse(any["terminal.t6"].exists, "a shell is not a Task of its own")
        app.typeKey("d", modifierFlags: .command)
        XCTAssertTrue(any["pane.2"].waitForExistence(timeout: 5), "⌘D starts a shell beside the agent")
        app.typeKey("[", modifierFlags: .command)
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(any["pane.2"].exists, "⌘W on the agent closes nothing")
        XCTAssertTrue(any["pane.1"].exists, "the window stays")
        app.typeKey("]", modifierFlags: .command)
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(waitForGone(any["pane.2"]), "⌘W closes the shell")
    }

    @MainActor
    func testATabClosesItsTaskAfterAsking() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        let close = any["tab.close.t1"]
        XCTAssertTrue(close.waitForExistence(timeout: 5))
        close.click()
        let confirm = app.buttons["closetask.confirm"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        confirm.click()
        XCTAssertTrue(waitForGone(any["tab.t1"]))
    }

    @MainActor
    func testASidebarRowClosesItsTaskAfterAsking() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        let row = any["terminal.t1"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.hover()
        let close = app.buttons["row.close.t1"]
        XCTAssertTrue(close.waitForExistence(timeout: 5))
        close.click()
        let confirm = app.buttons["closetask.confirm"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        confirm.click()
        XCTAssertTrue(waitForGone(row))
    }

    @MainActor
    func testGoToTaskTakesTheKeyboardFromTheTerminal() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["pane.1"].waitForExistence(timeout: 5))
        app.typeKey("p", modifierFlags: .command)
        XCTAssertTrue(app.textFields["goto.field"].waitForExistence(timeout: 5))
        // Typed at once, without a click: the field must have the keyboard.
        app.typeText("fix")
        XCTAssertEqual(app.textFields["goto.field"].value as? String, "fix")
        app.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(waitForGone(app.textFields["goto.field"]))
        XCTAssertTrue(any["tab.t1"].isSelected)
    }

    @MainActor
    func testOtherTerminalsStayClosedUntilOpened() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["terminal.t1"].waitForExistence(timeout: 5))
        for id in ["t3", "t4", "t5"] { XCTAssertFalse(any["terminal.\(id)"].exists, id) }
        any["tasks.others"].firstMatch.click()
        XCTAssertTrue(any["terminal.t4"].waitForExistence(timeout: 5))
        XCTAssertTrue(any["terminal.t4"].label.contains("Terminal t4"))
        XCTAssertTrue(any["terminal.t5"].label.contains("dependency-digest run"))
    }

    @MainActor
    func testARepoStartsItsCheckoutAgentFromTheSidebar() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["repo.app.app"].waitForExistence(timeout: 5))
        XCTAssertTrue(any["terminal.t1"].exists)
        any["repo.start.app.app"].click()
        // The started agent opens as a tab titled for the checkout: repo and branch.
        let tab = any.matching(NSPredicate(format: "identifier BEGINSWITH 'tab.' AND label CONTAINS 'app main'"))
        XCTAssertTrue(tab.firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(any["repo.open.app.app"].waitForExistence(timeout: 5))
        XCTAssertFalse(any["repo.start.app.app"].exists)
    }

    @MainActor
    func testCaptureTakesTheKeyboardEachTimeItOpens() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["pane.1"].waitForExistence(timeout: 5))
        let field = app.textFields["capture.field"]
        for line in ["first", "second"] {
            app.menuBars.menuItems["Capture…"].click()
            XCTAssertTrue(field.waitForExistence(timeout: 5))
            // Typed at once, without a click: the field must have the keyboard.
            app.typeText(line)
            XCTAssertTrue(waitFor(field, valueContaining: line), line)
            app.typeKey(.escape, modifierFlags: [])
            XCTAssertTrue(waitForGone(field))
        }
    }

    @MainActor
    func testCaptureStartsATaskWithTheWindowClosed() throws {
        let app = launch()
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
        app.menuBars.menuItems["Capture from Anywhere"].click()
        let field = app.textFields["capture.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("Write the release notes")
        app.typeKey(.return, modifierFlags: .command)
        let title = app.textFields["task.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.value as? String, "Write the release notes")
    }

    @MainActor
    func testRequestChangesTypesTheCommentIntoTheAgent() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d1"].click()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        app.typeKey("r", modifierFlags: .command)
        let comment = app.textFields["detail.comment"]
        XCTAssertTrue(comment.waitForExistence(timeout: 5))
        comment.click()
        comment.typeText("Use a 30 s margin\r")
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
        app.typeKey("f", modifierFlags: [.command, .shift])
        let agent = app.descendants(matching: .any)["terminal.t1"]
        XCTAssertTrue(agent.waitForExistence(timeout: 5))
        agent.click()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Use a 30 s margin"))
    }

    @MainActor
    func testRetryRunsTheFailedRoutineAgain() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d5"].click()
        XCTAssertTrue(app.buttons["detail.retry"].waitForExistence(timeout: 5))
        app.typeKey("r", modifierFlags: .command)
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d5"]))
    }

    @MainActor
    func testTurnOnOpensAWorkspacesRoutines() throws {
        let app = launch()
        app.typeKey("o", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["routine.dependency-digest"].waitForExistence(timeout: 5))
        // The demo's Personal has one Routine in its file and its gate off.
        let on = app.buttons["routines.on.Personal"]
        XCTAssertTrue(on.waitForExistence(timeout: 5))
        on.click()
        XCTAssertTrue(waitForGone(on))
        // The Routines page became Bots (ADR 0086); the header's mode switch goes Home.
        app.buttons["mode.home"].click()
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testATaskStartsFromTheSheetWithItsLoop() throws {
        let app = launch()
        app.typeKey("n", modifierFlags: [.command, .shift])
        let title = app.textFields["task.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        title.click()
        title.typeText("Write the session tests")
        let loop = app.checkBoxes["task.loop"].exists ? app.checkBoxes["task.loop"] : app.switches["task.loop"]
        XCTAssertTrue(loop.waitForExistence(timeout: 5))
        loop.click()
        XCTAssertTrue(app.descendants(matching: .any)["task.check"].waitForExistence(timeout: 5))
        app.buttons["task.start"].click()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["Loop 0/3"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testAReaderLinkOpensTheLinkedDocument() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.typeKey("r", modifierFlags: [.command, .shift])
        let plan = app.descendants(matching: .any)["reader.file.PLAN.md"]
        XCTAssertTrue(plan.waitForExistence(timeout: 10))
        plan.click()
        let link = app.webViews["reader"].links.firstMatch
        XCTAssertTrue(link.waitForExistence(timeout: 10))
        link.click()
        XCTAssertTrue(waitFor(app.staticTexts["reader.title"], labelContaining: "TASK.md"))
        XCTAssertTrue(app.webViews["reader"].staticTexts["fix-login"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testTheArrowKeysMoveThroughTheReadersImages() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.typeKey("r", modifierFlags: [.command, .shift])
        let kind = app.radioGroups["reader.kind"]
        XCTAssertTrue(kind.waitForExistence(timeout: 10))
        kind.radioButtons["Images"].click()
        let first = app.descendants(matching: .any)["reader.file.screenshots/login.png"]
        XCTAssertTrue(first.waitForExistence(timeout: 10))
        first.click()
        let title = app.staticTexts["reader.title"]
        XCTAssertTrue(waitFor(title, labelContaining: "login.png"))
        app.typeKey(.rightArrow, modifierFlags: [])
        XCTAssertTrue(waitFor(title, labelContaining: "session.png"))
        app.typeKey(.leftArrow, modifierFlags: [])
        XCTAssertTrue(waitFor(title, labelContaining: "login.png"))
    }

    @MainActor
    func testTheReaderTakesTheKeyboardWhenItOpens() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.typeKey("r", modifierFlags: [.command, .shift])
        let kind = app.radioGroups["reader.kind"]
        XCTAssertTrue(kind.waitForExistence(timeout: 10))
        // No click into the Reader: esc reaches it, not the terminal.
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(waitForGone(kind))
    }

    @MainActor
    func testTheReaderGroupsFilesUnderTheirFolder() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.typeKey("r", modifierFlags: [.command, .shift])
        let kind = app.radioGroups["reader.kind"]
        XCTAssertTrue(kind.waitForExistence(timeout: 10))
        kind.radioButtons["Images"].click()
        let folder = app.descendants(matching: .any)["reader.folder.screenshots"]
        XCTAssertTrue(folder.waitForExistence(timeout: 10))
        let file = app.descendants(matching: .any)["reader.file.screenshots/login.png"]
        XCTAssertTrue(file.waitForExistence(timeout: 5))
        folder.disclosureTriangles.firstMatch.click()
        XCTAssertTrue(waitForGone(file))
        folder.disclosureTriangles.firstMatch.click()
        XCTAssertTrue(file.waitForExistence(timeout: 5))
    }

    @MainActor
    func testTheReaderShowsATasksPlan() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.typeKey("r", modifierFlags: [.command, .shift])
        let plan = app.descendants(matching: .any)["reader.file.PLAN.md"]
        XCTAssertTrue(plan.waitForExistence(timeout: 10))
        plan.click()
        XCTAssertTrue(app.webViews["reader"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testATasksLoopStarts() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.buttons["focus.loop"].click()
        let start = app.buttons["loop.start"]
        XCTAssertTrue(start.waitForExistence(timeout: 5))
        start.click()
        XCTAssertTrue(app.buttons["Loop 0/3"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testChatAnswersThePrompt() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        app.typeKey("c", modifierFlags: [.command, .shift])
        let allow = app.buttons["chat.option.Allow"]
        XCTAssertTrue(allow.waitForExistence(timeout: 10))
        allow.click()
        XCTAssertTrue(waitFor(pane, valueContaining: "(y/n) y"))
    }

    @MainActor
    func testTypingInFocusReachesTheTerminal() throws {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        pane.click()
        pane.typeText("y")
        // The demo's program echoes what it gets.
        XCTAssertTrue(waitFor(pane, valueContaining: "(y/n) y"))
    }

    @MainActor
    func testGoToTaskAndASecondPane() throws {
        let app = launch()
        app.typeKey("p", modifierFlags: .command)
        let field = app.textFields["goto.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("docs\r")
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        // ⌘\ adds the next Task beside it, and ⌘1 goes back to the first.
        app.typeKey("\\", modifierFlags: .command)
        XCTAssertTrue(app.descendants(matching: .any)["pane.2"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.descendants(matching: .any).matching(identifier: "focus.pane").count, 2)
        app.typeKey("1", modifierFlags: .command)
        app.menuBars.menuItems["Close Pane"].click()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["pane.2"]))
    }

    func waitFor(_ e: XCUIElement, valueContaining text: String) -> Bool {
        let p = NSPredicate(format: "value CONTAINS %@", text)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }

    func waitFor(_ e: XCUIElement, labelContaining text: String) -> Bool {
        let p = NSPredicate(format: "label CONTAINS %@", text)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }

    func waitForGone(_ e: XCUIElement) -> Bool {
        let p = NSPredicate(format: "exists == false")
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }
}
