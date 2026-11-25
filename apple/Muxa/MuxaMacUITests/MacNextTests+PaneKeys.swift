import XCTest

/// The Pane menu's keys in each state the user reached Focus from: they once
/// stayed disabled after Home. t1 has its shell t6 as pane 2, so a split is pane 3.
extension MacNextTests {
    @MainActor
    private func splitAndClose(_ app: XCUIApplication, _ state: String) {
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["pane.2"].waitForExistence(timeout: 5), state)
        XCTAssertFalse(any["pane.3"].exists, state)
        app.typeKey("d", modifierFlags: .command)
        XCTAssertTrue(any["pane.3"].waitForExistence(timeout: 5), "⌘D splits \(state)")
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(waitForGone(any["pane.3"]), "⌘W closes the new shell \(state)")
    }

    @MainActor
    func testSplitWorksAfterHomeTheReaderAndABotChat() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        splitAndClose(app, "in Focus")

        app.typeKey("h", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 5))
        app.typeKey("f", modifierFlags: [.command, .shift])
        splitAndClose(app, "after Home")

        app.typeKey("r", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["reader.kind"].waitForExistence(timeout: 5))
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["reader.kind"]))
        splitAndClose(app, "after the Reader")

        openPrWatchsHome(app)
        let talk = app.descendants(matching: .any)["bot.terminal.t7"]
        XCTAssertTrue(talk.waitForExistence(timeout: 5))
        talk.click()
        XCTAssertTrue(app.descendants(matching: .any)["chat.assistant"].firstMatch.waitForExistence(timeout: 5))
        app.typeKey("f", modifierFlags: [.command, .shift])
        splitAndClose(app, "after a Bot chat")
    }

    @MainActor
    func testGoAndPaneKeysWorkAfterHome() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        let any = app.descendants(matching: .any)
        XCTAssertTrue(any["pane.2"].waitForExistence(timeout: 5))
        app.typeKey("h", modifierFlags: [.command, .shift])
        app.typeKey("f", modifierFlags: [.command, .shift])
        XCTAssertTrue(any["pane.2"].waitForExistence(timeout: 5))

        app.typeKey("r", modifierFlags: [.command, .shift])
        XCTAssertTrue(any["reader.kind"].waitForExistence(timeout: 5), "⇧⌘R opens the Reader")
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(waitForGone(any["reader.kind"]))

        app.typeKey("p", modifierFlags: .command)
        XCTAssertTrue(app.textFields["goto.field"].waitForExistence(timeout: 5), "⌘P opens Go to Task")
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(waitForGone(app.textFields["goto.field"]))

        app.typeKey("2", modifierFlags: .command)
        app.typeKey("w", modifierFlags: .command)
        XCTAssertTrue(waitForGone(any["pane.2"]), "⌘2 picks the shell, so ⌘W closes it")
    }
}
