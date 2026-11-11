import XCTest

/// Talk to Assistant (ADR 0054) on a `work` level 15 host, against the demo's `-demo-plans`
/// fixtures: the overall workspace is "assistant", and its root Ask session is `ask-assistant`.
final class TalkTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    /// The Ask session's pane opens, as after Ask Agent. A phone pushes it under the
    /// session's name; a tablet shows its view switch.
    @MainActor
    private func opensTheAssistantsChat(_ app: XCUIApplication) {
        let pane = isPhone ? app.navigationBars["ask-assistant"] : app.segmentedControls["view-mode"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10), "the overall assistant's Ask session")
    }

    /// Today's Talk to Assistant opens the overall assistant's Chat.
    @MainActor
    func testTodaysTalkOpensTheAssistantsChat() throws {
        let app = launchWorkInbox(["-demo-plans"])
        XCTAssertTrue(inboxSection(app, "today").waitForExistence(timeout: 5))
        tapWhenReady(app.buttons["today-talk"])
        opensTheAssistantsChat(app)
    }

    /// The `+` sheet's Talk to Assistant closes the sheet, then opens the same Chat.
    /// It shows only while the sheet is empty: a typed thought is never left unsaved.
    @MainActor
    func testThePlusSheetsTalkOpensTheChat() throws {
        let app = launchWorkInbox(["-demo-plans"])
        tapWhenReady(app.buttons["sidebar-capture"])
        let talk = app.buttons["capture-talk"]
        XCTAssertTrue(talk.waitForExistence(timeout: 5))
        tapWhenReady(talk)
        XCTAssertTrue(app.descendants(matching: .any)["capture-text"].firstMatch.waitForNonExistence(timeout: 5))
        opensTheAssistantsChat(app)
    }

    /// A level 14 host has no Talk to Assistant.
    @MainActor
    func testALevel14HostOffersNoTalk() throws {
        let app = launchWorkInbox(["-demo-plans", "-work14"])
        XCTAssertTrue(inboxSection(app, "today").waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["today-talk"].exists)
        tapWhenReady(app.buttons["sidebar-capture"])
        XCTAssertTrue(app.descendants(matching: .any)["capture-text"].firstMatch.waitForExistence(timeout: 5))
        XCTAssertFalse(app.buttons["capture-talk"].exists)
    }
}
