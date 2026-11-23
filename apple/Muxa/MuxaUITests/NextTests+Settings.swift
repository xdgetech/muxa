import XCTest

/// Settings against the demo's in-memory settings, Start Task's line, and Remove anyway.
extension NextTests {
    @MainActor
    func testAWorkspaceIsAdded() throws {
        let app = launch()
        showSettings(app)
        let add = app.buttons["settings.addworkspace"]
        XCTAssertTrue(add.waitForExistence(timeout: 10))
        add.tap()
        type("Lab", into: app.textFields["workspace.name"])
        type("/demo/lab", into: app.textFields["workspace.root"])
        app.buttons["editor.save"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings.workspace.Lab"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testTerminalColorsArePickedOnThisDevice() throws {
        let app = launch()
        showSettings(app)
        // The row is last, and a list loads its rows only once they scroll in.
        let list = app.descendants(matching: .any)["settings.list"].firstMatch
        XCTAssertTrue(list.waitForExistence(timeout: 10))
        let row = app.buttons["settings.terminal"]
        for _ in 0..<5 where !(row.exists && row.isHittable) { list.swipeUp() }
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        let dracula = app.buttons["terminal.theme.dracula"]
        XCTAssertTrue(dracula.waitForExistence(timeout: 5))
        XCTAssertTrue(app.descendants(matching: .any)["terminal.preview"].exists)
        dracula.tap()
        XCTAssertTrue(dracula.isSelected, "the picked theme is marked")
        // Back to the default, so the other tests see the usual colors.
        app.buttons["terminal.theme.muxa"].tap()
        XCTAssertTrue(app.buttons["terminal.theme.muxa"].isSelected)
    }

    @MainActor
    func testAConnectionWithASecretShowsSet() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.connections")
        XCTAssertTrue(app.descendants(matching: .any)["ws.note"].waitForExistence(timeout: 5))
        app.buttons["connection.add"].tap()
        type("notion", into: app.textFields["connection.name"])
        type("NOTION_TOKEN", into: app.textFields["connection.env"])
        type("not-a-real-token", into: app.secureTextFields["connection.secret"])
        app.buttons["editor.save"].tap()
        let row = app.descendants(matching: .any)["connection.notion"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(waitFor(row, labelContaining: "set"))
        XCTAssertFalse(row.label.contains("not-a-real-token"))
    }

    @MainActor
    func testAnMcpServerIsAdded() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.mcp")
        app.buttons["mcp.add"].tap()
        type("search", into: app.textFields["mcp.name"])
        type("npx", into: app.textFields["mcp.command"])
        type("search-mcp", into: app.textFields["mcp.args"])
        app.buttons["editor.save"].tap()
        let row = app.descendants(matching: .any)["mcp.search"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(row.label.contains("npx search-mcp"))
    }

    @MainActor
    func testARoutinesInstructionsAreEdited() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.routines")
        let row = app.descendants(matching: .any)["settings.routine.dependency-digest"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        let editor = app.textViews["routine.instructions"]
        scrollTo(editor, in: app)
        editor.tap()
        editor.typeText(" Skip dev dependencies.")
        app.buttons["editor.save"].tap()
        XCTAssertTrue(waitForGone(editor))
        row.tap()
        scrollTo(editor, in: app)
        XCTAssertTrue(waitFor(editor, valueContaining: "Skip dev dependencies."))
    }

    @MainActor
    func testStartTaskShowsWhatTheWorkspacesAgentsGet() throws {
        let app = launch()
        showTasks(app)
        app.buttons["home.newtask"].tap()
        XCTAssertTrue(app.textFields["task.title"].waitForExistence(timeout: 10))
        let line = app.staticTexts["task.tools"]
        scrollTo(line, in: app)
        XCTAssertEqual(line.label, "This Workspace's agents get: github, linear · MCP: docs")
        XCTAssertFalse(app.switches["task.connection.github"].exists)
    }

    @MainActor
    func testRemoveAnywayClosesATaskWhoseWorktreeHoldsWork() throws {
        let app = launch()
        let item = app.descendants(matching: .any)["item.d8"]
        scrollTo(item, in: app)
        item.tap()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        app.buttons["detail.main"].tap()
        // The alert's button and its action share the name.
        let force = app.alerts.buttons["Remove Anyway"].firstMatch
        XCTAssertTrue(force.waitForExistence(timeout: 5))
        force.tap()
        XCTAssertTrue(waitForGone(force))
        backToList(app)
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d8"]))
    }

    /// Settings: the phone's tab, or the gear at the foot of the tablet's sidebar.
    func showSettings(_ app: XCUIApplication) {
        show(app, tab: "Settings", segment: "home.settings")
    }

    /// The demo's `app` Workspace, on one of its tabs.
    func openWorkspace(_ app: XCUIApplication, tab: String) {
        showSettings(app)
        let row = app.descendants(matching: .any)["settings.workspace.app"]
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.tap()
        let button = app.buttons[tab]
        XCTAssertTrue(button.waitForExistence(timeout: 5))
        button.tap()
    }

    func type(_ text: String, into field: XCUIElement) {
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.tap()
        field.typeText(text)
    }

    /// A Form or List loads its rows lazily: swipe until the element is there.
    func scrollTo(_ e: XCUIElement, in app: XCUIApplication) {
        for _ in 0..<6 where !(e.exists && e.isHittable) {
            app.swipeUp()
        }
        XCTAssertTrue(e.waitForExistence(timeout: 5))
    }
}
