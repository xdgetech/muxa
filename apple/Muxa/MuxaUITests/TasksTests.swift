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
}
