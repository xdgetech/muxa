import XCTest

/// Bots on the phone and the tablet (ADR 0086): a Bot's page, its chat with
/// the proposal bar, New Bot and rename, against the demo.
extension NextTests {
    /// pr-watch's page opens on its talk; back from the talk, then its Home.
    @MainActor
    func openPrWatchsHome(_ app: XCUIApplication) {
        showBots(app)
        let bot = app.buttons["bot.app.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.tap()
        let talk = app.navigationBars["Talk to pr-watch"]
        XCTAssertTrue(talk.waitForExistence(timeout: 10))
        talk.buttons.element(boundBy: 0).tap()
        let home = app.buttons["bot.tab.home"]
        XCTAssertTrue(home.waitForExistence(timeout: 5))
        home.tap()
        XCTAssertTrue(app.staticTexts["bot.title"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testBotsShowPrWatchsHome() throws {
        let app = launch()
        showBots(app)
        XCTAssertTrue(app.buttons["bot.app.pr-watch"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.descendants(matching: .any)["routine.nightly-backup"].exists, "Routines without a Bot follow")
        openPrWatchsHome(app)
        XCTAssertTrue(app.descendants(matching: .any)["bot.item.d12"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.descendants(matching: .any)["bot.terminal.t8"].exists)
    }

    @MainActor
    func testAProposalIsAppliedFromABotsChat() throws {
        let app = launch()
        showBots(app)
        let bot = app.buttons["bot.app.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.tap()
        // The page lands on the talk, on the Bot page, not as a Task.
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["proposal.op.0"].waitForExistence(timeout: 5), "the proposal bar shows its ops")
        let apply = app.buttons["proposal.apply"].firstMatch
        apply.tap()
        XCTAssertTrue(waitForGone(apply), "Apply closes the item, and the bar with it")
    }

    @MainActor
    func testATalkIsNotAWorkspacesTask() throws {
        let app = launch()
        showTasks(app)
        XCTAssertTrue(app.descendants(matching: .any)["terminal.t1"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.descendants(matching: .any)["terminal.t7"].exists, "pr-watch's talk is on its page")
    }

    @MainActor
    func testNewBotOpensItsFirstChatOnItsPage() throws {
        let app = launch()
        showBots(app)
        let new = app.buttons["bots.new"]
        XCTAssertTrue(new.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(new, enabled: true))
        new.tap()
        type("Deps", into: app.textFields["newbot.name"])
        XCTAssertTrue(waitFor(app.textFields["newbot.name"], valueContaining: "deps"), "typed capitals become lowercase")
        type("Keeps the dependencies up to date", into: app.textFields["newbot.description"])
        app.buttons["newbot.create"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testABotIsRenamedOnItsPage() throws {
        let app = launch()
        openPrWatchsHome(app)
        app.buttons["bot.tab.settings"].tap()
        let field = app.textFields["bot.rename.name"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.tap()
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 12))
        field.typeText("reviews")
        app.buttons["bot.rename"].tap()
        XCTAssertTrue(waitFor(app.staticTexts["bot.title"], labelContaining: "reviews"))
    }

    @MainActor
    func testABotWithALiveTalkLandsOnIt() throws {
        let app = launch()
        showBots(app)
        let bot = app.buttons["bot.app.scout"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.tap()
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 10),
                      "scout's talk still works, so its page opens on it")
        XCTAssertTrue(app.navigationBars["Talk to scout"].exists)
    }

    /// pr-watch's talk reported `done`: it waits for the user, and the page still lands on it.
    @MainActor
    func testATalkWhoseTurnIsDoneIsLandedOn() throws {
        let app = launch()
        showBots(app)
        let bot = app.buttons["bot.app.pr-watch"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.tap()
        XCTAssertTrue(app.navigationBars["Talk to pr-watch"].waitForExistence(timeout: 10))
    }

    /// Opening a Bot again lands on its talk again, though it was left on Home.
    @MainActor
    func testABotLeftOnHomeOpensOnItsTalkAgain() throws {
        let app = launch()
        openPrWatchsHome(app)
        let scout = app.buttons["bot.app.scout"]
        // The phone's list is behind the page; the tablet's sidebar shows it.
        if !scout.isHittable { app.navigationBars.buttons.element(boundBy: 0).tap() }
        XCTAssertTrue(scout.waitForExistence(timeout: 5))
        scout.tap()
        XCTAssertTrue(app.navigationBars["Talk to scout"].waitForExistence(timeout: 10))
        let pr = app.buttons["bot.app.pr-watch"]
        for _ in 0..<3 where !pr.isHittable { app.navigationBars.buttons.element(boundBy: 0).tap() }
        pr.tap()
        XCTAssertTrue(app.navigationBars["Talk to pr-watch"].waitForExistence(timeout: 10), "pr-watch opens on its talk again")
    }

    @MainActor
    func testATalkStartsFromABotsHome() throws {
        let app = launch()
        openPrWatchsHome(app)
        let field = app.textFields["bot.home.talk"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.tap()
        field.typeText("What waits for me today?\n")
        XCTAssertTrue(app.descendants(matching: .any)["bot.conversation"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testChatEmptiesItsFieldAfterASend() throws {
        let app = launch()
        showBots(app)
        let bot = app.buttons["bot.app.scout"]
        XCTAssertTrue(bot.waitForExistence(timeout: 10))
        bot.tap()
        let field = app.textFields["chat.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        let send = app.buttons["chat.send"]
        // Return sends.
        field.tap()
        field.typeText("First question\n")
        XCTAssertTrue(waitForEmpty(field), "the field empties after Return: \(field.value ?? "")")
        XCTAssertTrue(waitFor(send, enabled: false))
        // So does the button.
        field.tap()
        field.typeText("Second question")
        XCTAssertTrue(waitFor(send, enabled: true))
        send.tap()
        XCTAssertTrue(waitForEmpty(field), "the field empties after Send: \(field.value ?? "")")
        XCTAssertTrue(waitFor(send, enabled: false))
    }

    /// An empty text field shows its placeholder as its value.
    func waitForEmpty(_ field: XCUIElement) -> Bool {
        let p = NSPredicate(format: "value == '' OR value == placeholderValue OR value == nil")
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: field)], timeout: 10) == .completed
    }

    func waitFor(_ e: XCUIElement, enabled: Bool) -> Bool {
        let p = NSPredicate(format: "isEnabled == %@", NSNumber(value: enabled))
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }
}
