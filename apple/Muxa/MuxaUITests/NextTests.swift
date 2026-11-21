import XCTest

/// The app against its in-app v2 demo server (`-demo`).
final class NextTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

    override func setUp() {
        super.setUp()
        XCUIDevice.shared.orientation = .portrait
    }

    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    func testHomeShowsTheInboxAndWhoNeedsYou() throws {
        let app = launch()
        for id in ["d1", "d2", "d3", "d4", "d5", "d6"] {
            XCTAssertTrue(app.descendants(matching: .any)["item.\(id)"].exists, id)
        }
        XCTAssertTrue(app.buttons["needsyou.t2"].exists)
    }

    @MainActor
    func testACaptureShowsItsFieldsAndGoesOnTop() throws {
        let app = launch()
        let field = app.textFields["capture.field"]
        field.tap()
        field.typeText("reply Sam about the launch #personal due today")
        XCTAssertTrue(app.buttons["#Personal, remove"].waitForExistence(timeout: 5))
        app.buttons["capture.add"].tap()
        let top = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'item.'")).firstMatch
        XCTAssertTrue(waitFor(top, labelContaining: "Sam about the launch"))
        XCTAssertTrue(top.label.contains("Reply"))
    }

    @MainActor
    func testApproveTakesTheReviewOutOfTheList() throws {
        let app = launch()
        app.descendants(matching: .any)["item.d1"].tap()
        XCTAssertTrue(app.staticTexts["detail.title"].waitForExistence(timeout: 5))
        XCTAssertEqual(app.staticTexts["detail.title"].label, "Review fix-login")
        app.buttons["detail.main"].tap()
        XCTAssertTrue(waitForGone(app.descendants(matching: .any)["item.d1"]))
    }

    @MainActor
    func testARoutineRunsNow() throws {
        let app = launch()
        app.segmentedControls["home.part"].buttons["Routines"].tap()
        let row = app.descendants(matching: .any)["routine.release-notes"]
        XCTAssertTrue(row.waitForExistence(timeout: 5))
        row.tap()
        let run = app.buttons["routine.run"]
        XCTAssertTrue(run.waitForExistence(timeout: 5))
        run.tap()
        // The detail (on a phone it covers the list) shows the run started.
        let started = NSPredicate(format: "isEnabled == false")
        XCTAssertEqual(XCTWaiter().wait(for: [expectation(for: started, evaluatedWith: run)], timeout: 10), .completed)
        XCTAssertTrue(app.staticTexts["Running since"].exists)
    }

    @MainActor
    func testNeedsYouOpensTheWaitingAgentInFocus() throws {
        let app = launch()
        app.buttons["needsyou.t2"].tap()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Allow edits to README.md"))
    }

    @MainActor
    func testTypingInFocusReachesTheTerminal() throws {
        let app = launch()
        app.buttons["mode.focus"].tap()
        let shell = app.descendants(matching: .any)["terminal.t3"]
        XCTAssertTrue(shell.waitForExistence(timeout: 5))
        shell.tap()
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "$"))
        pane.tap()
        pane.typeText("echo hi\n")
        XCTAssertTrue(waitFor(pane, valueContaining: "echo hi"))
        // The Inbox panel opens beside the terminal.
        app.buttons["focus.inbox"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["item.d1"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testGoToTaskOpensItInFocus() throws {
        let app = launch()
        app.typeKey("p", modifierFlags: .command)
        let field = app.textFields["goto.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("docs\n")
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "Allow edits to README.md"))
    }

    @MainActor
    func testDisconnectReturnsToTheHostsAndAHostCanBeAdded() throws {
        let app = launch()
        app.buttons["home.host"].tap()
        app.buttons["Disconnect"].tap()
        let add = app.buttons["next.addhost"]
        XCTAssertTrue(add.waitForExistence(timeout: 5))
        add.tap()
        let address = app.textFields["host-address"]
        XCTAssertTrue(address.waitForExistence(timeout: 5))
        app.textFields["host-name"].tap()
        app.textFields["host-name"].typeText("Nowhere")
        address.tap()
        address.typeText("127.0.0.1")
        app.textFields["host-user"].tap()
        app.textFields["host-user"].typeText("me")
        // Port 22 on the simulator's Mac may answer; a closed port fails at once.
        app.buttons["Connect"].tap()
        XCTAssertTrue(app.buttons["Nowhere"].waitForExistence(timeout: 5))
        let link = app.staticTexts["next.link"]
        XCTAssertTrue(XCTWaiter().wait(for: [expectation(
            for: NSPredicate(format: "NOT (label IN %@)", ["Connecting…", "Not connected"]), evaluatedWith: link)],
            timeout: 20) == .completed)
    }

    private func waitFor(_ e: XCUIElement, valueContaining text: String) -> Bool {
        let p = NSPredicate(format: "value CONTAINS %@", text)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }

    private func waitFor(_ e: XCUIElement, labelContaining text: String) -> Bool {
        let p = NSPredicate(format: "label CONTAINS %@", text)
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }

    private func waitForGone(_ e: XCUIElement) -> Bool {
        let p = NSPredicate(format: "exists == false")
        return XCTWaiter().wait(for: [expectation(for: p, evaluatedWith: e)], timeout: 10) == .completed
    }
}
