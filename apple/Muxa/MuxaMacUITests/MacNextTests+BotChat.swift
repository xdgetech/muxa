import AppKit
import XCTest

/// pr-watch's talk on its Bot page: its text copies, and Close asks about the chat.
extension MacNextTests {
    @MainActor
    private func openTalk(_ app: XCUIApplication) {
        app.typeKey("b", modifierFlags: [.command, .shift])
        let talk = app.descendants(matching: .any)["bot.terminal.t7"]
        XCTAssertTrue(talk.waitForExistence(timeout: 10))
        talk.click()
        XCTAssertTrue(app.descendants(matching: .any)["chat.assistant"].firstMatch.waitForExistence(timeout: 5))
    }

    @MainActor
    func testAChatMessageIsSelectedAndCopied() throws {
        let app = launch()
        openTalk(app)
        NSPasteboard.general.clearContents()
        let user = app.staticTexts["chat.user"].firstMatch
        XCTAssertTrue(user.waitForExistence(timeout: 5))
        user.doubleClick()
        app.typeKey("c", modifierFlags: .command)
        let word = NSPasteboard.general.string(forType: .string) ?? ""
        XCTAssertFalse(word.isEmpty, "a double click selects a word of the message")
        XCTAssertTrue("Every night, list the PRs nobody touched for a week.".contains(word))
    }

    @MainActor
    func testAChatMessageAndTheWholeChatAreCopiedFromTheMenu() throws {
        let app = launch()
        openTalk(app)
        NSPasteboard.general.clearContents()
        app.descendants(matching: .any)["chat.assistant"].firstMatch.rightClick()
        let message = app.menuItems["Copy Message"]
        XCTAssertTrue(message.waitForExistence(timeout: 5))
        message.click()
        XCTAssertEqual(NSPasteboard.general.string(forType: .string),
                       "I proposed a Routine **stale-prs** that runs at 21:00. Apply it to turn it on.")
        app.staticTexts["chat.user"].firstMatch.rightClick()
        let chat = app.menuItems["Copy Chat"]
        XCTAssertTrue(chat.waitForExistence(timeout: 5))
        chat.click()
        XCTAssertEqual(NSPasteboard.general.string(forType: .string), """
            You: Every night, list the PRs nobody touched for a week.

            I proposed a Routine **stale-prs** that runs at 21:00. Apply it to turn it on.
            """)
    }

    @MainActor
    func testClosingAChatNamesTheBotAndItsProposal() throws {
        let app = launch()
        openTalk(app)
        app.buttons["bot.chat.close"].click()
        let confirm = app.buttons["closetask.confirm"]
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        XCTAssertEqual(confirm.label, "Close Chat")
        let alert = app.sheets.firstMatch.exists ? app.sheets.firstMatch : app.dialogs.firstMatch
        XCTAssertTrue(alert.staticTexts["Close this chat with pr-watch?"].exists)
        let says = alert.staticTexts.matching(NSPredicate(format: "value CONTAINS 'Inbox item is marked Done' OR label CONTAINS 'Inbox item is marked Done'"))
        XCTAssertTrue(says.firstMatch.exists, "the proposal waiting in the Inbox is named")
        confirm.click()
        XCTAssertTrue(waitForGone(app.buttons["bot.chat.t7"]))
    }
}
