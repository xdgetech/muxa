import XCTest

/// The New Task sheet on a `work` level 2 host: the workspace's repos, a branch named
/// after the title, and ask sessions (ADR 0031). `-work1` is an older host.
final class AskTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    /// Opens the demo, then the sidebar's New Task with the repository fields shown.
    @MainActor
    private func openNewTask(_ arguments: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-mode", "sessions"] + arguments
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        showSidebar(app)
        tapWhenReady(app.buttons["sidebar-new-task"])
        XCTAssertTrue(app.textFields["record-title"].waitForExistence(timeout: 5))
        return app
    }

    @MainActor
    private func type(_ app: XCUIApplication, title: String, goal: String) {
        let titleField = app.textFields["record-title"]
        tapWhenReady(titleField)
        titleField.typeText(title)
        let goalField = app.textViews["record-goal"].exists ? app.textViews["record-goal"] : app.textFields["record-goal"]
        tapWhenReady(goalField)
        goalField.typeText(goal)
    }

    @MainActor
    private func showRepoFields(_ app: XCUIApplication) {
        let toggle = app.switches["record-with-repo"]
        XCTAssertTrue(toggle.waitForExistence(timeout: 5))
        if (toggle.value as? String) != "1" { tapWhenReady(toggle) }
        // A tap can land on the label; the switch itself sits at the trailing edge.
        if (toggle.value as? String) != "1" { toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap() }
        XCTAssertTrue(app.descendants(matching: .any)["record-repo-choice"].firstMatch.waitForExistence(timeout: 5))
    }

    /// A drag, as a finger scrolls. `swipeUp()` on the New Task form does not scroll it
    /// while the phone keyboard is up; a drag does.
    @MainActor
    private func reveal(_ app: XCUIApplication, _ element: XCUIElement) {
        let window = app.windows.firstMatch
        for _ in 0..<4 where !(element.exists && element.isHittable) {
            window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
                .press(forDuration: 0.05, thenDragTo: window.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.2)))
        }
    }

    @MainActor
    private func pickRepo(_ app: XCUIApplication, search: String, name: String, listed: String? = nil) {
        let choice = app.descendants(matching: .any)["record-repo-choice"].firstMatch
        // The form scrolls to the new fields first; on a phone the keyboard may still cover them.
        let hittable = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"), object: choice)
        if XCTWaiter.wait(for: [hittable], timeout: 10) != .completed { reveal(app, choice) }
        tapWhenReady(choice)
        if let listed { XCTAssertTrue(app.buttons["repo-\(listed)"].waitForExistence(timeout: 5), listed) }
        let field = app.searchFields.firstMatch
        tapWhenReady(field)
        field.typeText(search)
        tapWhenReady(app.buttons["repo-\(name)"])
        XCTAssertTrue(waitFor(app.descendants(matching: .any)["record-repo-choice"].firstMatch, labelOrValue: name))
    }

    @MainActor
    private func waitFor(_ element: XCUIElement, labelOrValue text: String) -> Bool {
        let predicate = NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", text, text)
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [exp], timeout: 5) == .completed
    }

    /// Repos under the workspace root show by their relative path, the one outside it
    /// with `~`. Without a branch, the host names one after the title.
    @MainActor
    func testARepoIsPickedByItsRelativeNameAndNeedsNoBranch() throws {
        let app = openNewTask()
        type(app, title: "Index the docs", goal: "Build a search index.")
        showRepoFields(app)
        XCTAssertTrue(waitFor(app.descendants(matching: .any)["record-repo-choice"].firstMatch, labelOrValue: "app"),
                      "the shown session's repo")
        // A repo outside the root shows with `~`.
        pickRepo(app, search: "core", name: "libs/core", listed: "~/forks/tmux")
        let branch = app.textFields["record-branch"]
        XCTAssertTrue(branch.waitForExistence(timeout: 5))
        XCTAssertEqual(branch.placeholderValue, "Named after the title")
        XCTAssertFalse(app.descendants(matching: .any)["record-error"].exists, "no branch is needed")
        save("record-repo-relative")
        tapWhenReady(app.buttons["record-start"])
        // The new task's session opens once the sheet has closed.
        XCTAssertTrue(app.navigationBars.descendants(matching: .any)["index-the-docs"].firstMatch.waitForExistence(timeout: 10))
    }

    /// Ask opens the agent of a new manual session in the repo; the repo header asks too.
    @MainActor
    func testAskOpensAManualSessionInTheRepo() throws {
        let app = openNewTask()
        showRepoFields(app)
        pickRepo(app, search: "app", name: "app")
        let ask = app.buttons["record-ask"]
        reveal(app, ask)
        XCTAssertTrue(app.staticTexts["Ask runs the workspace's agent in the main checkout. No task, no worktree, no record."].exists)
        save("record-ask")
        tapWhenReady(ask)
        XCTAssertTrue(app.textFields["record-title"].waitForNonExistence(timeout: 5), "the sheet closes")
        XCTAssertTrue(app.navigationBars.descendants(matching: .any)["ask-app-1"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.descendants(matching: .any)["pane"].firstMatch.waitForExistence(timeout: 10), "pane first")

        showSidebar(app)
        XCTAssertTrue(app.staticTexts["ask-app-1"].waitForExistence(timeout: 5), "listed under Sessions")
        // Past Back's animation: a drag before it ends scrolls the header away.
        let header = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == true"), object: app.buttons["ask-app"])
        _ = XCTWaiter.wait(for: [header], timeout: 3)
        reveal(app, app.buttons["ask-app"])
        tapWhenReady(app.buttons["ask-app"])
        XCTAssertTrue(app.navigationBars.descendants(matching: .any)["ask-app-2"].firstMatch.waitForExistence(timeout: 10))
    }

    /// A host before `work` level 2 still needs a branch and has no Ask.
    @MainActor
    func testAnOlderHostNeedsABranchAndHasNoAsk() throws {
        let app = openNewTask(["-work1"])
        type(app, title: "Index the docs", goal: "Build a search index.")
        showRepoFields(app)
        let branch = app.textFields["record-branch"]
        XCTAssertTrue(branch.waitForExistence(timeout: 5))
        XCTAssertEqual(branch.placeholderValue, "feature/name")
        let error = app.descendants(matching: .any)["record-error"].firstMatch
        reveal(app, error)
        XCTAssertTrue(error.label.contains("Name the branch."), error.label)
        XCTAssertFalse(app.buttons["record-ask"].exists)
        XCTAssertFalse(app.buttons["record-start"].isEnabled)
        tapWhenReady(app.buttons["Cancel"])
        XCTAssertTrue(app.textFields["record-title"].waitForNonExistence(timeout: 5))
        // No repo header offers Ask either.
        XCTAssertEqual(app.buttons.matching(NSPredicate(format: "identifier BEGINSWITH 'ask-'")).count, 0)
    }
}
