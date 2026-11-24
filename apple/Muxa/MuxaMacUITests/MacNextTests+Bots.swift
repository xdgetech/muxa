import XCTest

/// The Bots page (ADR 0086) against the demo: pr-watch's Home, the
/// proposal bar above its talk in Focus, and New Bot.
extension MacNextTests {
    @MainActor
    func testBotsOpensOnPrWatchsHome() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let bot = app.descendants(matching: .any)["bot.app.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.click()
        XCTAssertTrue(app.staticTexts["bot.title"].waitForExistence(timeout: 5))
        for section in ["bot.home.needsyou", "bot.home.inprogress", "bot.home.planned"] {
            XCTAssertTrue(app.descendants(matching: .any)[section].exists, section)
        }
        XCTAssertTrue(app.descendants(matching: .any)["bot.item.d12"].exists)
        XCTAssertTrue(app.descendants(matching: .any)["bot.terminal.t8"].exists)
        // The Routines without a Bot are a section of Bots.
        XCTAssertTrue(app.descendants(matching: .any)["routine.dependency-digest"].exists)
        app.typeKey("4", modifierFlags: .command)
        XCTAssertTrue(app.descendants(matching: .any)["bot.files.memory"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["bot.files.restore.1"].exists)
    }

    @MainActor
    func testAnItemOnABotsHomeOpensInHome() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let row = app.descendants(matching: .any)["bot.item.d10"]
        XCTAssertTrue(row.waitForExistence(timeout: 10))
        row.click()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["detail.title"].value as? String ?? app.staticTexts["detail.title"].label,
                       "Approve the docs PR #57?")
    }

    @MainActor
    func testAProposalIsAppliedFromItsChatOnTheBotPage() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let talk = app.descendants(matching: .any)["bot.terminal.t7"]
        XCTAssertTrue(talk.waitForExistence(timeout: 10))
        talk.click()
        // The talk opens on the Bot page, on Chat, not in Focus (ADR 0086).
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["bot.chat.t7"].exists)
        let bar = app.descendants(matching: .any)["proposal.bar"]
        XCTAssertTrue(bar.waitForExistence(timeout: 5))
        app.typeKey(.return, modifierFlags: .command)
        XCTAssertTrue(waitForGone(bar))
        app.typeKey("h", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["item.d12"].exists, "the Inbox item closed with the bar")
    }

    @MainActor
    func testATalkIsNotInFocus() throws {
        let app = launch()
        app.typeKey("f", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.descendants(matching: .any)["terminal.t1"].exists, "a plain Task is in Focus")
        XCTAssertFalse(app.descendants(matching: .any)["terminal.t7"].exists, "pr-watch's talk is not")
        XCTAssertFalse(app.descendants(matching: .any)["tab.t7"].exists)
    }

    @MainActor
    func testNewBotStartsItsFirstChatOnItsPage() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let new = app.buttons["bots.new"]
        XCTAssertTrue(new.waitForExistence(timeout: 10))
        new.click()
        type("deps", into: app.textFields["newbot.name"])
        type("Keeps the dependencies up to date", into: app.textFields["newbot.description"])
        app.buttons["newbot.create"].click()
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.descendants(matching: .any)["bot.app.deps"].exists)
        XCTAssertTrue(app.buttons["bot.tab.chats"].isSelected)
    }
}

/// Editing a Bot's Routine stays on the Bot page: the editor is a sheet over
/// the main window, not the Settings window.
extension MacNextTests {
    @MainActor
    func testARoutineIsEditedInASheetOnTheBotPage() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        XCTAssertTrue(app.descendants(matching: .any)["bot.app.pr-watch"].waitForExistence(timeout: 10))
        app.typeKey("3", modifierFlags: .command)
        let edit = app.buttons["bot.routine.edit.scan"]
        XCTAssertTrue(edit.waitForExistence(timeout: 5))
        edit.click()
        let main = app.windows.firstMatch
        let timing = main.sheets.firstMatch.popUpButtons["routine.timing"]
        XCTAssertTrue(timing.waitForExistence(timeout: 5), "the editor is a sheet of the main window")
        timing.click()
        app.menuItems["On review"].click()
        main.sheets.firstMatch.buttons["editor.save"].click()
        let row = app.descendants(matching: .any)["bot.routine.scan"]
        let line = row.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS 'On review'")).firstMatch
        XCTAssertTrue(line.waitForExistence(timeout: 10), "the Routine's line says it runs on review")
        XCTAssertFalse(app.descendants(matching: .any)["settings.workspace.app"].exists, "Settings did not open")
    }
}

/// A Bot's settings are a tab of its page (⌘5), saved in place.
extension MacNextTests {
    @MainActor
    func testABotsSettingsAreEditedOnItsPage() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        XCTAssertTrue(app.descendants(matching: .any)["bot.app.pr-watch"].waitForExistence(timeout: 10))
        app.typeKey("5", modifierFlags: .command)
        let save = app.buttons["bot.settings.save"]
        XCTAssertTrue(save.waitForExistence(timeout: 5))
        XCTAssertFalse(save.isEnabled, "nothing changed yet")
        XCTAssertTrue(app.descendants(matching: .any)["bot.access"].exists, "Access moved here from Files")
        XCTAssertTrue(app.buttons["editor.remove"].exists)
        let line = app.textFields["bot.description"]
        line.click()
        line.typeText(", and the release threads")
        XCTAssertTrue(save.isEnabled)
        save.click()
        XCTAssertTrue(waitFor(app.descendants(matching: .any)["bot.app.pr-watch"], labelContaining: "pr-watch"))
        XCTAssertTrue(app.staticTexts.matching(NSPredicate(format: "label CONTAINS 'release threads'")).firstMatch
            .waitForExistence(timeout: 5))
        app.typeKey("4", modifierFlags: .command)
        XCTAssertTrue(app.buttons["bot.files.editsoul"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["bot.access"].exists, "Files keeps Soul and Memory only")
    }
}

/// Renaming a Bot keeps its page open under the new name (ADR 0089).
extension MacNextTests {
    @MainActor
    func testABotIsRenamedOnItsPage() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        XCTAssertTrue(app.descendants(matching: .any)["bot.app.pr-watch"].waitForExistence(timeout: 10))
        app.typeKey("5", modifierFlags: .command)
        let field = app.textFields["bot.rename.name"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        let rename = app.buttons["bot.rename"]
        XCTAssertFalse(rename.isEnabled, "the name is unchanged")
        field.doubleClick()
        field.typeKey("a", modifierFlags: .command)
        field.typeText("Reviews")
        XCTAssertTrue(waitFor(field, valueContaining: "reviews"), "typed capitals become lowercase")
        rename.click()
        XCTAssertTrue(app.descendants(matching: .any)["bot.app.reviews"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["bot.title"].value as? String ?? app.staticTexts["bot.title"].label, "reviews")
    }
}

/// A Bot's page first opens on its live talk, else on Home with a field to start one (ADR 0086).
extension MacNextTests {
    @MainActor
    func testABotWithALiveTalkLandsOnIt() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let bot = app.descendants(matching: .any)["bot.app.scout"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.click()
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["bot.tab.chats"].isSelected)
        XCTAssertTrue(app.buttons["bot.chat.t9"].isSelected)
    }

    @MainActor
    func testABotWithoutATalkStartsOneFromItsHome() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let bot = app.descendants(matching: .any)["bot.app.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.click()
        let field = app.textFields["bot.home.talk"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["bot.tab.home"].isSelected)
        field.click()
        field.typeText("What waits for me today?\r")
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["bot.tab.chats"].isSelected)
    }
}

/// ⌘T on a Bot with a live talk asks in muxa's popup: Return starts a new chat.
extension MacNextTests {
    @MainActor
    func testNewChatOffersToContinueATalk() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let bot = app.descendants(matching: .any)["bot.app.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.click()
        app.buttons["bot.tab.home"].click()
        app.typeKey("t", modifierFlags: .command)
        let start = app.buttons["bot.chat.new"]
        XCTAssertTrue(start.waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'bot.chat.continue.'")).count > 0)
        app.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(waitForGone(start))
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 10))
    }
}

/// Chat's field is empty and Send off after a send, by Return or the button.
extension MacNextTests {
    @MainActor
    func testChatEmptiesItsFieldAfterASend() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let bot = app.descendants(matching: .any)["bot.app.scout"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.click()
        let field = app.textFields["chat.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        let send = app.buttons["chat.send"]
        field.click()
        field.typeText("First question\r")
        XCTAssertTrue(waitForEmpty(field), "the field empties after Return")
        XCTAssertFalse(send.isEnabled)
        field.click()
        field.typeText("Second question")
        XCTAssertTrue(send.isEnabled)
        send.click()
        XCTAssertTrue(waitForEmpty(field), "the field empties after Send")
        XCTAssertFalse(send.isEnabled)
    }

    @MainActor
    func testShiftReturnPutsANewLineInChat() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: .command)
        let bot = app.descendants(matching: .any)["bot.app.scout"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.click()
        let field = app.textFields["chat.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.click()
        field.typeText("First line")
        field.typeKey(.return, modifierFlags: .shift)
        field.typeText("Second line")
        XCTAssertEqual(field.value as? String, "First line\nSecond line", "Shift+Return does not send")
        field.typeKey(.return, modifierFlags: [])
        XCTAssertTrue(waitForEmpty(field), "Return sends both lines")
    }

    func waitForEmpty(_ field: XCUIElement) -> Bool {
        let p = NSPredicate(format: "value == '' OR value == nil")
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: field)], timeout: 10) == .completed
    }
}

/// A Bot has no daily item limit (ADR 0094); the Inbox deletes old items in
/// one step (ADR 0095). Each test keeps a screenshot for the review.
extension MacNextTests {
    @MainActor
    func testABotsSettingsHaveNoDailyItems() throws {
        let app = launch()
        openWorkspace(app, tab: "ws.tab.bots")
        let bot = app.descendants(matching: .any)["settings.bot.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 5))
        bot.click()
        XCTAssertTrue(app.textFields["bot.description"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.textFields["bot.daily"].exists)
        keep(app, "bot-settings")
    }

    @MainActor
    func testABotsHomeSaysItsItemsTodayWithNoLimit() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: [.command, .shift])
        let bot = app.descendants(matching: .any)["bot.app.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.click()
        XCTAssertTrue(app.staticTexts["bot.title"].waitForExistence(timeout: 5))
        let perDay = NSPredicate(format: "label CONTAINS 'items a day' OR label CONTAINS ' of '")
        XCTAssertEqual(app.staticTexts.matching(perDay).count, 0)
        keep(app, "bot-home")
    }

    @MainActor
    func testDeleteOlderAsksThenClearsTheInbox() throws {
        let app = launch()
        let more = app.descendants(matching: .any)["inbox.more"]
        XCTAssertTrue(more.waitForExistence(timeout: 5))
        more.click()
        let submenu = app.menuItems["Delete Older Than"]
        XCTAssertTrue(submenu.waitForExistence(timeout: 5))
        submenu.hover()
        let oneDay = app.menuItems["1 day"]
        XCTAssertTrue(oneDay.waitForExistence(timeout: 5))
        oneDay.click()
        // The demo's items are from 2025, so every one is older than a day.
        let delete = app.buttons["inbox.deleteOlder.confirm"]
        XCTAssertTrue(delete.waitForExistence(timeout: 5))
        keep(app, "inbox-delete-older")
        delete.click()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
    }

    @MainActor
    private func keep(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.windows.firstMatch.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
