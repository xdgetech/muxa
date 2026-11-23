import XCTest

/// The Bots page (ADR 0086) against the demo: pr-watch's Home, the
/// proposal bar above its talk in Focus, and New Bot.
extension MacNextTests {
    @MainActor
    func testBotsOpensOnPrWatchsHome() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: [.command, .shift])
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
        app.typeKey("b", modifierFlags: [.command, .shift])
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
        app.typeKey("b", modifierFlags: [.command, .shift])
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
        app.typeKey("b", modifierFlags: [.command, .shift])
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
