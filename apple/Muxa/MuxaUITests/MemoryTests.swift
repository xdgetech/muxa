import XCTest

/// Workspace memory on a `work` level 14 host (ADR 0053), against the demo's `-demo-memory`
/// fixtures: "work" reviews each addition and its file changed three hours ago. The done
/// Task t-keys01 "Rotate the API keys" proposes i-0000e001, a 4 KiB text of 102 numbered
/// lines; the done Task t-relnot's proposal i-0000e002 was approved.
final class MemoryTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launchMemory(_ arguments: [String] = []) -> XCUIApplication {
        launchWorkInbox(["-demo-memory"] + arguments)
    }

    @MainActor
    private func element(_ app: XCUIApplication, _ id: String) -> XCUIElement {
        app.descendants(matching: .any)[id].firstMatch
    }

    @MainActor
    private func openItem(_ app: XCUIApplication, _ id: String) {
        showInboxList(app)
        let row = app.buttons["item-row-\(id)"]
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(app.staticTexts["item-question"].waitForExistence(timeout: 5))
    }

    /// Opens a Task's detail from Tasks mode; a closed Task has no session to open instead.
    @MainActor
    private func openTask(_ app: XCUIApplication, _ id: String) {
        let row = app.buttons["task-\(id)"]
        // Back from an item, Tasks may still be shown, scrolled so its mode picker is under the bar.
        if !row.exists { showMode(app, 1) }
        XCTAssertTrue(waitForSidebarRow(app, row))
        tapWhenReady(row)
        XCTAssertTrue(element(app, "task-detail").waitForExistence(timeout: 5))
    }

    /// Scrolls the Task detail until `target` is on screen; a phone shows less of it.
    @MainActor
    private func revealInTask(_ app: XCUIApplication, _ target: XCUIElement) -> Bool {
        for _ in 0..<6 where !(target.exists && target.isHittable) {
            app.collectionViews["task-detail"].swipeUp()
        }
        return target.exists
    }

    /// The item shows every line of the 4 KiB text as plain text, with Approve, Reject and Later.
    @MainActor
    func testAMemoryItemShowsItsWholeText() throws {
        let app = launchMemory()
        openItem(app, "i-0000e001")
        let question = app.staticTexts["item-question"]
        XCTAssertEqual(question.label, "Rotate the API keys proposes to add to the workspace memory")
        XCTAssertTrue(element(app, "item-memory-workspace").label.contains("work"))
        let text = element(app, "item-memory-text")
        XCTAssertTrue(text.waitForExistence(timeout: 5))
        let lines = text.label.components(separatedBy: "\n").filter { $0.hasPrefix("Note ") }
        XCTAssertEqual(lines.count, 102, "every line of the 4 KiB fixture")
        XCTAssertEqual(lines.first, "Note 001: rotate keys with make rotate.")
        XCTAssertEqual(lines.last, "Note 102: rotate keys with make rotate.")
        XCTAssertEqual(app.links.count, 0, "no live link")
        XCTAssertTrue(app.buttons["item-answer-approve"].exists)
        XCTAssertTrue(app.buttons["item-answer-reject"].exists)
        XCTAssertTrue(app.buttons["item-answer-later"].exists)
        save("item-memory")
    }

    /// Approve names the workspace once more; then the Task's detail says it was added.
    @MainActor
    func testApproveNamesTheWorkspaceAndTheTaskSaysAdded() throws {
        let app = launchMemory()
        openItem(app, "i-0000e001")
        tapWhenReady(app.buttons["item-answer-approve"])
        let confirm = app.buttons["memory-confirm"].firstMatch
        XCTAssertTrue(confirm.waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["It goes into the memory of work. Every later Task there reads it."].exists,
                      "the workspace once more")
        save("item-memory-confirm")
        confirm.tap()
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000e001"].waitForNonExistence(timeout: 5), "answered")

        openTask(app, "t-keys01")
        // The mode sits in the details, above the line.
        let mode = element(app, "task-workspace-memory")
        XCTAssertTrue(revealInTask(app, mode))
        XCTAssertTrue(mode.label.contains("Review · changed"), mode.label)
        XCTAssertFalse(mode.label.contains("3h"), "approve changed the file just now")
        let line = element(app, "task-memory")
        XCTAssertTrue(revealInTask(app, line))
        XCTAssertTrue(line.label.contains("Added to the workspace memory"), line.label)
    }

    /// A proposal that waits opens its item from the Task's detail; Reject then shows there.
    @MainActor
    func testTheTaskDetailOpensAWaitingProposalAndShowsReject() throws {
        let app = launchMemory()
        openTask(app, "t-keys01")
        let mode = element(app, "task-workspace-memory")
        XCTAssertTrue(revealInTask(app, mode))
        XCTAssertTrue(mode.label.contains("Review · changed 3h ago"), mode.label)
        let line = element(app, "task-memory")
        XCTAssertTrue(revealInTask(app, line))
        XCTAssertTrue(line.label.contains("Proposed to the workspace memory · Waiting in the Inbox"), line.label)
        save("task-memory-waiting")
        tapWhenReady(line)
        XCTAssertTrue(element(app, "item-memory-text").waitForExistence(timeout: 5))

        tapWhenReady(app.buttons["item-answer-reject"])
        showInboxList(app)
        XCTAssertTrue(app.buttons["item-row-i-0000e001"].waitForNonExistence(timeout: 5))
        openTask(app, "t-keys01")
        XCTAssertTrue(revealInTask(app, line))
        XCTAssertTrue(line.label.contains("Proposed to the workspace memory · Rejected"), line.label)
    }

    /// An approved proposal from earlier shows on its Task.
    @MainActor
    func testAnApprovedProposalShowsOnItsTask() throws {
        let app = launchMemory()
        openTask(app, "t-relnot")
        let line = element(app, "task-memory")
        XCTAssertTrue(revealInTask(app, line))
        XCTAssertTrue(line.label.contains("Added to the workspace memory"), line.label)
    }

    /// A level 13 host shows none of it: no memory items, no line, no mode.
    @MainActor
    func testALevel13HostShowsNoMemory() throws {
        let app = launchMemory(["-work13"])
        XCTAssertFalse(app.buttons["item-row-i-0000e001"].exists, "no memory items")
        openTask(app, "t-keys01")
        XCTAssertTrue(revealInTask(app, app.buttons["task-status"]))
        XCTAssertFalse(element(app, "task-memory").exists)
        XCTAssertFalse(element(app, "task-workspace-memory").exists)
    }
}
