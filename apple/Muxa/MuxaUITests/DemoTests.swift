import XCTest

/// Drives the app against its in-app demo bridge (`-demo`): no host needed.
final class DemoTests: XCTestCase {
    override class func setUp() {
        super.setUp()
        AnimationIdle.bound
    }

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

    /// A fresh shell opened from a tall agent pane shows its first row: the view
    /// does not keep the agent's scroll position or cursor.
    @MainActor
    func testFreshShellShowsItsTopAfterTheAgent() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }

        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        pane.typeKey("6", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %7 100x60"))
        sleep(1)
        let window = app.windows.firstMatch.frame
        // The agent's prompt is near the bottom, so the view scrolled down to it.
        XCTAssertLessThan(pane.frame.minY, window.minY)

        pane.typeKey("j", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %8 100x60"))
        sleep(1)
        XCTAssertGreaterThanOrEqual(pane.frame.minY, window.minY, "the shell's first row is scrolled out of view")
        save("landscape-fresh-shell")
    }

    /// Japanese input: the pane gets the converted text once, never the kana on the way.
    /// Needs the simulator's Japanese (Romaji) keyboard; skipped without it.
    @MainActor
    func testJapaneseInputIsSentWhenCommitted() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        // A tap, not Cmd+R: after a hardware key, iPadOS shows no software keyboard.
        app.buttons["Agent"].firstMatch.tap()
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))

        // The Japanese keyboard's key that switches between Roman letters and kana.
        let kana = app.keyboards.keys["Switch to Roman keys"]
        try XCTSkipUnless(kana.waitForExistence(timeout: 3), "no Japanese keyboard on this simulator")
        if app.keyboards.keys["Fullwidth"].exists { kana.tap() }
        for key in ["n", "i", "h", "o", "n", "g", "o"] { app.keyboards.keys[key].tap() }
        // The candidate bar above the keys.
        let candidate = app.descendants(matching: .any).matching(NSPredicate(format: "label == '日本語'")).firstMatch
        XCTAssertTrue(candidate.waitForExistence(timeout: 5))
        XCTAssertFalse(waitFor(pane, containing: "$ に"), "kana reached the pane before the commit")
        save("japanese-composing")
        candidate.tap()
        XCTAssertTrue(waitFor(pane, containing: "$ 日本語"))
    }

    /// The bar above the on-screen keyboard sends the keys it lacks; Ctrl is one-shot.
    @MainActor
    func testKeyBarSendsKeysTheKeyboardLacks() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        // A tap, not Cmd+R: after a hardware key, iPadOS shows no software keyboard.
        app.buttons["Agent"].firstMatch.tap()
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))

        tapWhenReady(app.buttons["keybar-escape"])
        XCTAssertTrue(waitFor(pane, containing: "<Escape>"))
        app.buttons["keybar-up"].tap()
        XCTAssertTrue(waitFor(pane, containing: "<Up>"))
        // Ctrl is one-shot. The chord wins also when the keyboard composes letters (kana).
        app.buttons["keybar-control"].tap()
        XCTAssertTrue(app.buttons["keybar-control"].isSelected)
        app.keyboards.keys["c"].tap()
        XCTAssertTrue(waitFor(pane, containing: "<C-c>"))
        XCTAssertFalse(app.buttons["keybar-control"].isSelected)
        // The input method kept nothing of the chord: a bar key would send it first.
        app.buttons["keybar-escape"].tap()
        XCTAssertTrue(waitFor(pane, containing: "<C-c><Escape>"))
        save("keybar")
    }

    /// The Reader lists uncommitted changes first; a diff reads like GitHub's and
    /// follows the file.
    @MainActor
    func testChangesShowTheirDiffs() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["Documents, 3 changed files"].waitForExistence(timeout: 5))

        composer.typeKey("d", modifierFlags: [.command, .shift])
        let parser = app.descendants(matching: .any)["change-src/parser.rs"].firstMatch
        XCTAssertTrue(parser.waitForExistence(timeout: 5))
        parser.tap()
        let web = app.webViews.firstMatch
        let added = web.staticTexts.containing(NSPredicate(format: "label CONTAINS 'return Some(Vec::new());'")).firstMatch
        XCTAssertTrue(added.waitForExistence(timeout: 10))
        // Line numbers from the hunk header: the new line is 12.
        XCTAssertTrue(web.staticTexts["12"].exists)
        let update = web.staticTexts.containing(NSPredicate(format: "label CONTAINS 'checked by the agent'")).firstMatch
        XCTAssertTrue(update.waitForExistence(timeout: 10))
        save("reader-diff")
        XCUIDevice.shared.orientation = .landscapeLeft
        save("reader-diff-landscape")
        XCUIDevice.shared.orientation = .portrait

        tapWhenReady(app.descendants(matching: .any)["change-assets/logo.png"].firstMatch)
        XCTAssertTrue(app.staticTexts["Binary File"].waitForExistence(timeout: 5))
        app.buttons["Done"].tap()
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
    }

    /// Images, ignored ones included, are a folder tree with the newest run open;
    /// Previous and Next step through one folder.
    @MainActor
    func testImagesAreATreeAndStepThroughTheirFolder() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))

        composer.typeKey("d", modifierFlags: [.command, .shift])
        // Images come after the Markdown: scroll the sidebar to them.
        let login = app.descendants(matching: .any)["image-build/screenshots/en/login.png"].firstMatch
        XCTAssertTrue(app.descendants(matching: .any)["change-src/parser.rs"].firstMatch.waitForExistence(timeout: 5))
        scrollToReveal(login, in: app.collectionViews.firstMatch)
        tapWhenReady(login)
        XCTAssertTrue(app.descendants(matching: .any)["image-view"].firstMatch.waitForExistence(timeout: 5))
        let position = app.staticTexts["image-position"]
        XCTAssertTrue(position.waitForExistence(timeout: 5))
        XCTAssertEqual(position.label, "2 of 2")
        save("reader-image")
        app.buttons["image-previous"].tap()
        XCTAssertTrue(app.navigationBars["home.png"].waitForExistence(timeout: 5))
        XCTAssertEqual(position.label, "1 of 2")

        // Older runs stay closed until opened.
        let ja = app.descendants(matching: .any)["image-build/screenshots/ja/login.png"].firstMatch
        XCTAssertFalse(ja.exists)
        let folder = app.descendants(matching: .any)["image-folder-build/screenshots/ja"].firstMatch
        scrollToReveal(folder, in: app.collectionViews.firstMatch)
        tapWhenReady(folder)
        XCTAssertTrue(ja.waitForExistence(timeout: 5))
        app.buttons["Done"].tap()
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
    }

    /// An agent elsewhere that starts waiting shows a banner; tapping it opens
    /// that session.
    @MainActor
    func testAWaitingAgentElsewhereIsAnnounced() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo", "-demo-alerts"]
        app.launch()
        XCTAssertTrue(app.navigationBars["feature-auth"].waitForExistence(timeout: 10))
        let banner = app.buttons["agent-alert"]
        XCTAssertTrue(banner.waitForExistence(timeout: 10))
        XCTAssertTrue(banner.label.contains("fix-login: needs your input"), banner.label)
        save("agent-alert")
        banner.tap()
        XCTAssertTrue(app.navigationBars["fix-login"].waitForExistence(timeout: 5))
        XCTAssertTrue(banner.waitForNonExistence(timeout: 5))
    }

    /// Closing the shown task opens the next one; a refused close says why and
    /// leaves the task open.
    @MainActor
    func testClosingTasks() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        XCTAssertTrue(app.descendants(matching: .any)["composer"].firstMatch.waitForExistence(timeout: 10))
        // The bar carries the shown session's name.
        let title = { (name: String) in app.navigationBars[name] }
        XCTAssertTrue(title("feature-auth").waitForExistence(timeout: 5))

        func closeShown(_ choice: String) {
            app.buttons["More"].tap()
            tapWhenReady(app.buttons["Close Task…"])
            let button = app.buttons[choice]
            XCTAssertTrue(button.waitForExistence(timeout: 5))
            button.tap()
        }

        closeShown("Close")
        XCTAssertTrue(title("fix-login").waitForExistence(timeout: 10))

        closeShown("Close")
        let alert = app.alerts["Can't Close the Task"]
        XCTAssertTrue(alert.waitForExistence(timeout: 5))
        XCTAssertTrue(alert.staticTexts.containing(NSPredicate(format: "label CONTAINS 'not merged'")).firstMatch.exists)
        save("close-task-refused")
        alert.buttons["OK"].tap()
        XCTAssertTrue(title("fix-login").exists)

        closeShown("Close and Keep Worktree")
        XCTAssertTrue(title("docs-site").waitForExistence(timeout: 10))
    }

    /// Cmd+N starts a task in the shown session's repo; a failure stays in the
    /// sheet, a success opens the new session.
    @MainActor
    func testANewTaskOpensItsSession() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()
        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))

        composer.typeKey("n", modifierFlags: .command)
        let branch = app.textFields["task-branch"]
        XCTAssertTrue(branch.waitForExistence(timeout: 5))
        XCTAssertTrue(waitForFocus(branch))
        branch.typeText("taken")
        app.buttons["task-start"].tap()
        let error = app.descendants(matching: .any)["task-error"].firstMatch
        XCTAssertTrue(error.waitForExistence(timeout: 5))
        XCTAssertTrue(error.label.contains("already exists"), error.label)
        save("new-task-error")

        branch.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: 5) + "feat/search")
        XCTAssertFalse(error.exists, "editing clears the error")
        app.buttons["task-start"].tap()
        XCTAssertTrue(app.navigationBars.descendants(matching: .any)["feat-search"].firstMatch.waitForExistence(timeout: 10))
        save("new-task-opened")
        XCTAssertTrue(branch.waitForNonExistence(timeout: 5), "the sheet closes")
    }

    /// A permission and a question the adapter knows show as a card, and its
    /// buttons send the option's keys to the pane.
    @MainActor
    func testApprovalCardAnswersWithTheOptionsKeys() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()

        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        let card = app.descendants(matching: .any)["approval"].firstMatch

        composer.tap()
        composer.typeText("/allow")
        app.buttons["send"].tap()
        XCTAssertTrue(card.waitForExistence(timeout: 5))
        XCTAssertTrue(card.staticTexts["Allow Bash?"].exists)
        XCTAssertTrue(card.staticTexts["rm -rf build"].exists)
        XCTAssertTrue(card.buttons["approval-terminal"].exists)
        save("approval-permission")
        card.buttons["approval-Allow"].tap()
        XCTAssertTrue(card.waitForNonExistence(timeout: 5))

        composer.tap()
        composer.typeText("/ask")
        app.buttons["send"].tap()
        XCTAssertTrue(card.waitForExistence(timeout: 5))
        XCTAssertTrue(card.staticTexts["Which format should the config use?"].exists)
        card.buttons["approval-JSON"].tap()
        XCTAssertTrue(card.waitForNonExistence(timeout: 5))

        // The keys went to the pane itself.
        composer.typeKey("r", modifierFlags: .command)
        let pane = app.descendants(matching: .any)["pane"].firstMatch
        XCTAssertTrue(waitFor(pane, containing: "<1>"))
        XCTAssertTrue(waitFor(pane, containing: "<2>"))
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
        // One switch for the session: Chat and Agent are the agent pane, Shell this one.
        let segments = app.segmentedControls["view-mode"]
        XCTAssertTrue(segments.buttons["Shell"].isSelected)
        segments.buttons["Chat"].tap()
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
        segments.buttons["Agent"].tap()
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        segments.buttons["Shell"].tap()
        XCTAssertTrue(waitFor(pane, containing: "pane %2 100x30"))
        assertToolbarDoesNotOverlap(app)
        XCUIDevice.shared.orientation = .landscapeLeft
        assertToolbarDoesNotOverlap(app)
        XCUIDevice.shared.orientation = .portrait
        pane.typeKey("4", modifierFlags: .command)
        XCTAssertTrue(waitFor(pane, containing: "pane %3 132x43"))
        XCTAssertFalse(app.descendants(matching: .any)["chat"].firstMatch.exists)
        XCTAssertFalse(segments.buttons["Chat"].exists)
        XCUIDevice.shared.orientation = .landscapeLeft
        pane.typeKey("1", modifierFlags: .command)
        // The pane was last shown as Agent, and opens that way again.
        XCTAssertTrue(waitFor(pane, containing: "pane %1 100x30"))
        segments.buttons["Chat"].tap()
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
        save("pretty-landscape")
        XCUIDevice.shared.orientation = .portrait
    }

    /// The Reader lists the plan first, renders tables, code and mermaid, follows
    /// links between documents, and shows the plan again when the agent edits it.
    @MainActor
    func testReaderRendersDocumentsAndFollowsChanges() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-demo"]
        app.launch()

        let composer = app.descendants(matching: .any)["composer"].firstMatch
        XCTAssertTrue(composer.waitForExistence(timeout: 10))
        composer.typeKey("d", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.staticTexts["Reader plan"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["README.md"].exists)

        // The plan opens first; the agent's edit shows up without a reload.
        let web = app.webViews.firstMatch
        XCTAssertTrue(web.staticTexts["Phases"].waitForExistence(timeout: 10))
        XCTAssertTrue(web.staticTexts["Updated by the agent while you read."].waitForExistence(timeout: 10))
        save("reader-plan")

        app.staticTexts["README.md"].tap()
        XCTAssertTrue(web.staticTexts["make sandbox"].waitForExistence(timeout: 10))
        // A web link asks before leaving; a relative link opens the document here.
        web.links["website"].tap()
        let open = app.buttons.containing(NSPredicate(format: "label BEGINSWITH 'Open example.com'")).firstMatch
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        // On iPad the question is a popover: tapping outside it says no.
        app.otherElements["PopoverDismissRegion"].firstMatch.tap()
        XCTAssertTrue(open.waitForNonExistence(timeout: 5))
        web.links["design"].tap()
        XCTAssertTrue(web.staticTexts["Design"].waitForExistence(timeout: 10))
        // The diagram is drawn, not shown as code.
        XCTAssertTrue(web.staticTexts["muxa bridge"].waitForExistence(timeout: 10))
        XCTAssertFalse(web.staticTexts["flowchart LR"].exists)
        save("reader-design")
        XCUIDevice.shared.orientation = .landscapeLeft
        save("reader-design-landscape")
        XCUIDevice.shared.orientation = .portrait

        app.buttons["Done"].tap()
        XCTAssertTrue(composer.waitForExistence(timeout: 5))
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
        app.buttons["Connect"].tap()
        XCTAssertTrue(app.buttons["host-Build box"].waitForExistence(timeout: 5))
    }

    /// Set MUXA_SCREENSHOT_DIR (TEST_RUNNER_MUXA_SCREENSHOT_DIR for xcodebuild) to keep screenshots.
    private func save(_ name: String) {
        guard let dir = ProcessInfo.processInfo.environment["MUXA_SCREENSHOT_DIR"] else { return }
        sleep(1)
        let png = XCUIScreen.main.screenshot().pngRepresentation
        try? png.write(to: URL(fileURLWithPath: dir).appendingPathComponent("\(name).png"))
    }

    /// The pane's bar: no two controls cover each other.
    @MainActor
    private func assertToolbarDoesNotOverlap(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        sleep(1)
        let bar = app.navigationBars.firstMatch
        let segmented = bar.segmentedControls.allElementsBoundByIndex.map { $0.frame }
        // A segment's own buttons sit inside its control; compare the control instead.
        let buttons = bar.buttons.allElementsBoundByIndex.filter { $0.isHittable }.map { $0.frame }
            .filter { b in !segmented.contains { $0.insetBy(dx: -2, dy: -2).contains(CGPoint(x: b.midX, y: b.midY)) } }
        let tops = (buttons + segmented).filter { !$0.isEmpty && $0.width < bar.frame.width / 2 }
        for (i, a) in tops.enumerated() {
            for b in tops[(i + 1)...] where !a.contains(b) && !b.contains(a) {
                XCTAssertFalse(a.insetBy(dx: 1, dy: 1).intersects(b.insetBy(dx: 1, dy: 1)), "\(a) overlaps \(b)", file: file, line: line)
            }
        }
    }

    /// Steps wait a bounded time for animations (`AnimationIdle`): wait until a menu
    /// or a rotation has put the element where a tap reaches it.
    @MainActor
    private func scrollToReveal(_ element: XCUIElement, in list: XCUIElement) {
        for _ in 0..<8 where !(element.exists && element.isHittable) {
            list.swipeUp()
        }
    }

    private func tapWhenReady(_ element: XCUIElement, file: StaticString = #filePath, line: UInt = #line) {
        let exp = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == true AND hittable == true"), object: element)
        XCTAssertEqual(XCTWaiter.wait(for: [exp], timeout: 5), .completed, file: file, line: line)
        element.tap()
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
