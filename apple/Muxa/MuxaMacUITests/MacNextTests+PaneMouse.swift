import AppKit
import XCTest

/// Text in a pane copies even when the program takes the mouse: fix-login (`t1`)
/// runs Codex in the demo, docs-audit (`t2`) Claude Code, which does not.
extension MacNextTests {
    @MainActor
    private func openPane(_ app: XCUIApplication, _ query: String) -> XCUIElement {
        app.typeKey("p", modifierFlags: .command)
        let field = app.textFields["goto.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 5))
        field.typeText("\(query)\r")
        let pane = app.descendants(matching: .any)["focus.pane"]
        XCTAssertTrue(pane.waitForExistence(timeout: 10))
        XCTAssertTrue(waitFor(pane, valueContaining: "https://example.com"))
        return pane
    }

    /// Drags over the pane's first lines with `modifiers` held, then copies with ⌘C.
    @MainActor
    private func dragAndCopy(_ app: XCUIApplication, _ pane: XCUIElement, _ modifiers: XCUIElement.KeyModifierFlags)
        -> String?
    {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("before", forType: .string)
        let start = pane.coordinate(withNormalizedOffset: CGVector(dx: 0.005, dy: 0.005))
        let end = pane.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.12))
        XCUIElement.perform(withKeyModifiers: modifiers) {
            start.press(forDuration: 0.2, thenDragTo: end)
        }
        app.typeKey("c", modifierFlags: .command)
        return NSPasteboard.general.string(forType: .string)
    }

    @MainActor
    func testOptionDragCopiesTextFromAPaneThatTakesTheMouse() throws {
        let app = launch()
        let pane = openPane(app, "fix")
        // Without a modifier the drag is Codex's.
        XCTAssertEqual(dragAndCopy(app, pane, []), "before")
        XCTAssertTrue(dragAndCopy(app, pane, .option)?.contains("Reading src/auth/session.ts") == true)
        XCTAssertTrue(dragAndCopy(app, pane, .shift)?.contains("Reading src/auth/session.ts") == true)
    }

    @MainActor
    func testADragCopiesTextFromAPaneThatLeavesTheMouse() throws {
        let app = launch()
        let pane = openPane(app, "docs")
        XCTAssertTrue(dragAndCopy(app, pane, [])?.contains("README links three pages") == true)
        XCTAssertTrue(dragAndCopy(app, pane, .option)?.contains("README links three pages") == true)
    }
}
