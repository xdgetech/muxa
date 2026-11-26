import XCTest

/// The Settings window (`⌘,`) against the demo's in-memory settings, Start
/// Task's line, and Remove anyway, as the iOS `NextTests`.
extension MacNextTests {
    @MainActor
    func testAWorkspaceIsAdded() throws {
        let app = launch()
        openSettings(app)
        let add = app.buttons["settings.addworkspace"]
        XCTAssertTrue(add.waitForExistence(timeout: 10))
        add.click()
        type("Lab", into: app.textFields["workspace.name"])
        type("/demo/lab", into: app.textFields["workspace.root"])
        app.buttons["editor.save"].click()
        XCTAssertTrue(app.descendants(matching: .any)["settings.workspace.Lab"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testAConnectionWithASecretShowsSet() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.connections")
        app.buttons["connection.add"].click()
        type("notion", into: app.textFields["connection.name"])
        type("NOTION_TOKEN", into: app.textFields["connection.env"])
        type("not-a-real-token", into: app.secureTextFields["connection.secret"])
        app.buttons["editor.save"].click()
        let row = app.descendants(matching: .any)["connection.notion"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        XCTAssertTrue(waitFor(row, labelContaining: "set"))
        XCTAssertFalse(row.label.contains("not-a-real-token"))
    }

    @MainActor
    func testAnMcpServerIsAdded() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.mcp")
        app.buttons["mcp.add"].click()
        type("search", into: app.textFields["mcp.name"])
        type("npx", into: app.textFields["mcp.command"])
        type("search-mcp", into: app.textFields["mcp.args"])
        app.buttons["editor.save"].click()
        XCTAssertTrue(app.descendants(matching: .any)["mcp.search"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testARoutinesInstructionsAreEdited() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.routines")
        let row = app.descendants(matching: .any)["settings.routine.dependency-digest"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.click()
        let editor = app.textViews["routine.instructions"]
        XCTAssertTrue(editor.waitForExistence(timeout: 5))
        editor.click()
        editor.typeText(" Skip dev dependencies.")
        app.buttons["editor.save"].click()
        XCTAssertTrue(waitForGone(editor))
        row.click()
        XCTAssertTrue(waitFor(editor, valueContaining: "Skip dev dependencies."))
    }

    @MainActor
    func testStartTaskShowsWhatTheWorkspacesAgentsGet() throws {
        let app = launch()
        app.typeKey("n", modifierFlags: [.command, .shift])
        let line = app.staticTexts["task.tools"]
        XCTAssertTrue(line.waitForExistence(timeout: 10))
        XCTAssertEqual(line.value as? String ?? line.label, "This Workspace's agents get: github, linear · MCP: docs")
    }

    @MainActor
    func testRemoveAnywayClosesATaskWhoseWorktreeHoldsWork() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d8"].click()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        app.typeKey(.return, modifierFlags: .command)
        let force = app.buttons["Remove Anyway"].firstMatch
        XCTAssertTrue(force.waitForExistence(timeout: 5))
        force.click()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d8"]))
    }

    @MainActor
    func testABotsDecideItemShowsItsLinkAndProposal() throws {
        let app = launch()
        let item = app.descendants(matching: .any)["item.d10"]
        XCTAssertTrue(item.waitForExistence(timeout: 10))
        item.click()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.descendants(matching: .any)["item.bot"].exists)
        XCTAssertTrue(app.buttons["detail.link.0"].exists)
        let proposal = app.staticTexts["detail.proposal"]
        XCTAssertTrue(proposal.waitForExistence(timeout: 5))
        XCTAssertEqual(proposal.value as? String ?? proposal.label, "Looks good to me. Merging with squash.")
    }

    @MainActor
    func testABotIsAddedWithItsSoul() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.bots")
        XCTAssertTrue(app.descendants(matching: .any)["settings.bot.pr-watch"].waitForExistence(timeout: 5))
        app.buttons["bot.add"].click()
        type("triage", into: app.textFields["bot.name"])
        let soul = app.textViews["bot.soul"]
        XCTAssertTrue(soul.waitForExistence(timeout: 5))
        soul.click()
        soul.typeText("I sort what came in overnight.")
        app.buttons["editor.save"].click()
        let row = app.descendants(matching: .any)["settings.bot.triage"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.click()
        XCTAssertTrue(waitFor(soul, valueContaining: "I sort what came in overnight."))
    }

    func openSettings(_ app: XCUIApplication) {
        app.typeKey(",", modifierFlags: .command)
    }

    /// A demo Workspace, `app` unless named, on one of its tabs.
    func openWorkspace(_ app: XCUIApplication, tab: String, name: String = "app") {
        openSettings(app)
        let row = app.descendants(matching: .any)["settings.workspace.\(name)"]
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()
        let button = app.buttons[tab]
        XCTAssertTrue(button.waitForExistence(timeout: 5))
        button.click()
    }

    func type(_ text: String, into field: XCUIElement) {
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.click()
        field.typeText(text)
    }
}

/// A Workspace lets the user's claude.ai connectors in (ADR 0087).
extension MacNextTests {
    @MainActor
    func testClaudeConnectorsAreAllowedInTheMcpTab() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.mcp")
        let toggle = app.descendants(matching: .any)["ws.claudeconnectors"].firstMatch
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        let control = app.checkBoxes["ws.claudeconnectors"].exists
            ? app.checkBoxes["ws.claudeconnectors"] : app.switches["ws.claudeconnectors"]
        control.click()
        XCTAssertTrue(waitFor(control, valueContaining: "1"))
    }
}

/// Removing a Bot that owns Routines moves them first, in one save.
extension MacNextTests {
    @MainActor
    func testABotWithRoutinesIsRemovedAndItsRoutinesMove() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.bots")
        let bot = app.descendants(matching: .any)["settings.bot.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 5))
        bot.click()
        XCTAssertTrue(app.descendants(matching: .any)["bot.remove.routine.scan"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.popUpButtons["bot.remove.moveto"].exists)
        app.buttons["editor.remove"].click()
        let confirm = app.buttons["editor.confirmremove"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        confirm.click()
        XCTAssertTrue(waitForGone(bot))
        app.buttons["ws.tab.routines"].click()
        let scan = app.descendants(matching: .any)["settings.routine.scan"]
        XCTAssertTrue(scan.waitForExistence(timeout: 5))
        XCTAssertFalse(scan.label.contains("pr-watch"), scan.label)
    }
}
