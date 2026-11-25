import XCTest

/// Saving the Reader's files on the Mac (ADR 0100): the file's own bytes, named as the file.
extension MacNextTests {
    @MainActor
    func testSaveKeepsAMarkdownFileAsItsSource() throws {
        let app = openReader()
        let plan = app.descendants(matching: .any)["reader.file.PLAN.md"]
        XCTAssertTrue(plan.waitForExistence(timeout: 10))
        plan.click()
        XCTAssertTrue(app.buttons["reader.save"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["reader.share"].exists)
        app.typeKey("s", modifierFlags: .command)
        let saved = try save(in: app, expecting: "PLAN.md")
        let text = try String(contentsOf: saved, encoding: .utf8)
        XCTAssertTrue(text.hasPrefix("# Plan\n"), "the Markdown source, not the rendered page")
    }

    @MainActor
    func testSaveKeepsAnImagesBytes() throws {
        let app = openReader()
        app.radioGroups["reader.kind"].radioButtons["Images"].click()
        // Folders start closed.
        let folder = app.descendants(matching: .any)["reader.folder.screenshots"]
        XCTAssertTrue(folder.waitForExistence(timeout: 10))
        folder.disclosureTriangles.firstMatch.click()
        let image = app.descendants(matching: .any)["reader.file.screenshots/login.png"]
        XCTAssertTrue(image.waitForExistence(timeout: 10))
        image.click()
        let button = app.buttons["reader.save"]
        XCTAssertTrue(button.waitForExistence(timeout: 10))
        button.click()
        let saved = try save(in: app, expecting: "login.png")
        let bytes = try Data(contentsOf: saved)
        XCTAssertEqual(Array(bytes.prefix(4)), [0x89, 0x50, 0x4E, 0x47], "a PNG")
    }

    @MainActor
    func testADiffSavesAsADiffFile() throws {
        let app = openReader()
        app.radioGroups["reader.kind"].radioButtons["Changes"].click()
        let change = app.descendants(matching: .any)["reader.file.src/auth/session.ts"]
        XCTAssertTrue(change.waitForExistence(timeout: 10))
        change.click()
        let button = app.buttons["reader.save"]
        XCTAssertTrue(button.waitForExistence(timeout: 10))
        button.click()
        let saved = try save(in: app, expecting: "session.ts.diff")
        let text = try String(contentsOf: saved, encoding: .utf8)
        XCTAssertTrue(text.contains("@@"), "the diff's text")
    }

    @MainActor
    private func openReader() -> XCUIApplication {
        let app = launch()
        app.descendants(matching: .any)["needsyou.t2"].click()
        XCTAssertTrue(app.descendants(matching: .any)["focus.pane"].waitForExistence(timeout: 10))
        app.typeKey("r", modifierFlags: [.command, .shift])
        XCTAssertTrue(app.radioGroups["reader.kind"].waitForExistence(timeout: 10))
        return app
    }

    /// Saves through the open save panel into a new folder, with the panel's own
    /// name, which must be `name`; returns the saved file.
    @MainActor
    private func save(in app: XCUIApplication, expecting name: String) throws -> URL {
        let sheet = app.sheets.firstMatch
        XCTAssertTrue(sheet.waitForExistence(timeout: 10))
        let field = sheet.textFields["saveAsNameTextField"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        XCTAssertEqual(field.value as? String, name)
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: folder) }
        // Go to Folder, as a user types a path into a save panel.
        app.typeKey("g", modifierFlags: [.command, .shift])
        app.typeText(folder.path + "\r")
        let save = sheet.buttons["Save"]
        XCTAssertTrue(save.waitForExistence(timeout: 5))
        save.click()
        XCTAssertTrue(waitForGone(sheet))
        let file = folder.appendingPathComponent(name)
        let written = NSPredicate { _, _ in FileManager.default.fileExists(atPath: file.path) }
        XCTAssertEqual(XCTWaiter().wait(for: [expectation(for: written, evaluatedWith: nil)], timeout: 10), .completed)
        return file
    }
}
