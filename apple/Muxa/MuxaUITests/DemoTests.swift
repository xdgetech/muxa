import XCTest

/// Drives the app against its in-app demo bridge (`-demo`): no host needed.
final class DemoTests: XCTestCase {
    @MainActor
    func testTypingReachesThePaneAndShortcutsSwitch() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()

        // The agent pane opens in Pretty; Cmd+R shows the terminal.
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        composer.typeKey("r", modifierFlags: .command)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        XCTAssertTrue(waitForFocus(pane))

        pane.typeText("hello")
        XCTAssertTrue(waitFor(pane, containing: "$ hello"))
        pane.typeKey("c", modifierFlags: .control)
        XCTAssertTrue(waitFor(pane, containing: "<C-c>"))

        // Cmd+J: the terminal side of the same session.
        pane.typeKey("j", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %2 100x30"))
        // Cmd+4: the fourth session in the sidebar's repo order, a bigger pane.
        pane.typeKey("4", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %3 132x43"))
        save("portrait-132x43")
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(waitFor(pane, containing: "pane %3 132x43"))
        save("landscape-132x43")
        XCUIDevice.shared.orientation = .portrait
    }

    /// A pane taller than a landscape view fits the width, scrolls, and follows the
    /// cursor until the user scrolls away from it.
    @MainActor
    func testLandscapeFitsWidthAndFollowsTheCursor() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }

        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        pane.typeKey("5", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %4 100x60"))

        // The grid stays exactly 100x60: the last row holds 99 cells and an X in column 100.
        let rows = (pane.value as? String ?? "").components(separatedBy: "\n")
        XCTAssertTrue(rows.contains { $0.count == 100 && $0.hasSuffix("X") }, "no 100-column last row")

        // Fit width: the grid spans the window and is taller than it.
        let window = app.windows.firstMatch.frame
        sleep(1)
        XCTAssertEqual(pane.frame.width, window.width, accuracy: 1)
        XCTAssertGreaterThan(pane.frame.height, window.height)
        // The cursor sits near the bottom, so the view scrolled down to it.
        XCTAssertLessThan(pane.frame.minY, window.minY)
        save("landscape-fit-width-100x60")

        // Scrolling up to read stops the following, even when output arrives.
        let followed = pane.frame.minY
        pane.swipeDown()
        sleep(1)
        let reading = pane.frame.minY
        XCTAssertGreaterThan(reading, followed + 50)
        pane.typeText("x")
        XCTAssertTrue(waitFor(pane, containing: "$ x"))
        sleep(1)
        XCTAssertEqual(pane.frame.minY, reading, accuracy: 1)
    }

    /// The agent pane opens as a chat; prompts reach the same pane; Raw is one key away.
    @MainActor
    func testPrettyViewSendsPromptsAndSwitchesToRaw() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()

        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Fix the failing test in the parser"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["cargo test -p parser"].exists)
        save("pretty-portrait")

        composer.tap()
        composer.typeText("hello")
        app.buttons["send"].tap()
        XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "You said")).firstMatch
            .waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["hello"].exists)

        // The prompt went into the pane itself.
        composer.typeKey("r", modifierFlags: .command)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "$ hello"))
        pane.typeKey("r", modifierFlags: .command)
        XCTAssertTrue(composer.waitForExistence(timeout: 5))

        // While the agent waits on a prompt, Send is off and the answer keys show.
        composer.tap()
        composer.typeText("/wait")
        app.buttons["send"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["waiting"].firstMatch.waitForExistence(timeout: 5))
        composer.typeText("x")
        XCTAssertFalse(app.buttons["send"].isEnabled)
        XCTAssertTrue(app.buttons["quick-1"].exists)
        save("pretty-waiting")

        // The shell side is always Raw, and a pane no adapter knows opens in Raw.
        composer.typeKey("j", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %2 100x30"))
        XCTAssertFalse(app.descendants(matching: .any)["view-mode"].firstMatch.exists)
        pane.typeKey("4", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %3 132x43"))
        XCTAssertFalse(app.descendants(matching: .any)["chat"].firstMatch.exists)
        XCUIDevice.shared.orientation = .landscapeLeft
        pane.typeKey("1", modifierFlags: .command)
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
        save("pretty-landscape")
        XCUIDevice.shared.orientation = .portrait
    }

    /// The pane gets the keyboard back after a sheet, and a new host can be saved.
    @MainActor
    func testFocusReturnsAfterTheSwitcherAndAHostCanBeAdded() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()

        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        composer.typeKey("r", modifierFlags: .command)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        XCTAssertTrue(waitForFocus(pane))
        save("portrait-pane")

        pane.typeKey("k", modifierFlags: .command)
        let close = app.buttons["Close"]
        XCTAssertTrue(close.waitForExistence(timeout: 5))
        close.tap()
        XCTAssertTrue(close.waitForNonExistence(timeout: 5))
        app.typeText("back")
        XCTAssertTrue(waitFor(pane, containing: "$ back"))

        // The sidebar: hosts, and the connected host's sessions.
        app.buttons["Show Sidebar"].firstMatch.tap()
        XCTAssertTrue(app.buttons["host-Demo"].waitForExistence(timeout: 5))
        // Sessions are grouped by repo, untagged ones last.
        XCTAssertTrue(app.staticTexts["app"].exists)
        XCTAssertTrue(app.staticTexts["docs"].exists)
        XCTAssertTrue(app.staticTexts["Other Sessions"].exists)
        save("sidebar")
        app.buttons["Add Host"].firstMatch.tap()
        let address = app.textFields["host-address"]
        XCTAssertTrue(address.waitForExistence(timeout: 5))
        app.textFields["host-name"].tap()
        app.typeText("Build box")
        address.tap()
        app.typeText("box.example")
        app.textFields["host-user"].tap()
        app.typeText("dev")
        save("host-editor")
        app.buttons["Save"].tap()
        XCTAssertTrue(app.buttons["host-Build box"].waitForExistence(timeout: 5))
    }

    /// Set MUXA_SCREENSHOT_DIR (TEST_RUNNER_MUXA_SCREENSHOT_DIR for xcodebuild) to keep screenshots.
    private func save(_ name: String) {
        guard let dir = ProcessInfo.processInfo.environment["MUXA_SCREENSHOT_DIR"] else { return }
        sleep(1)
        let png = XCUIScreen.main.screenshot().pngRepresentation
        try? png.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
    }

    private func waitForFocus(_ element: XCUIElement) -> Bool {
        let exp = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hasKeyboardFocus == true"), object: element)
        return XCTWaiter.wait(for: [exp], timeout: 5) == .completed
    }

    private func waitFor(_ element: XCUIElement, containing text: String) -> Bool {
        let predicate = NSPredicate(format: "value CONTAINS %@", text)
        let exp = XCTNSPredicateExpectation(predicate: predicate, object: element)
        return XCTWaiter.wait(for: [exp], timeout: 5) == .completed
    }
}
