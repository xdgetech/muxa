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
    func testAProposalIsAppliedFromItsTalkInFocus() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: [.command, .shift])
        let talk = app.descendants(matching: .any)["bot.terminal.t7"]
        XCTAssertTrue(talk.waitForExistence(timeout: 10))
        talk.click()
        let bar = app.descendants(matching: .any)["proposal.bar"]
        XCTAssertTrue(bar.waitForExistence(timeout: 5))
        app.buttons["proposal.apply"].click()
        XCTAssertTrue(waitForGone(bar))
        app.typeKey("h", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["item.d12"].exists, "the Inbox item closed with the bar")
    }

    @MainActor
    func testNewBotStartsItsFirstChat() throws {
        let app = launch()
        app.typeKey("b", modifierFlags: [.command, .shift])
        let new = app.buttons["bots.new"]
        XCTAssertTrue(new.waitForExistence(timeout: 10))
        new.click()
        type("deps", into: app.textFields["newbot.name"])
        type("Keeps the dependencies up to date", into: app.textFields["newbot.description"])
        app.buttons["newbot.create"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.typeKey("b", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.descendants(matching: .any)["bot.app.deps"].waitForExistence(timeout: 5))
    }
}
