import MuxaProtocol
import Testing

@testable import MuxaSession

struct KeyMapTests {
    typealias M = KeyMap.Modifiers

    @Test func specialKeysAreNamed() {
        let cases: [(KeyMap.Special, M, String)] = [
            (.up, [], "Up"),
            (.enter, [], "Enter"),
            (.enter, [.shift], "S-Enter"),
            (.tab, [.shift], "BTab"),
            (.backspace, [], "BSpace"),
            (.left, [.control, .option], "C-M-Left"),
            (.function(5), [], "F5"),
        ]
        for (key, mods, name) in cases {
            #expect(KeyMap.map(special: key, characters: "", modifiers: mods) == .keys([name]))
        }
    }

    @Test func chordsUseTmuxNames() {
        #expect(KeyMap.map(special: nil, characters: "c", modifiers: [.control]) == .keys(["C-c"]))
        #expect(KeyMap.map(special: nil, characters: "b", modifiers: [.option]) == .keys(["M-b"]))
        #expect(KeyMap.map(special: nil, characters: "A", modifiers: [.control, .shift]) == .keys(["C-A"]))
        #expect(KeyMap.map(special: nil, characters: " ", modifiers: [.control]) == .keys(["C-Space"]))
    }

    @Test func plainCharactersAreText() {
        #expect(KeyMap.map(special: nil, characters: "a", modifiers: []) == .text("a"))
        #expect(KeyMap.map(special: nil, characters: "é", modifiers: [.shift]) == .text("é"))
    }

    @Test func commandAndUnnamedKeysStayInTheApp() {
        #expect(KeyMap.map(special: nil, characters: "k", modifiers: [.command]) == KeyMap.Output.none)
        #expect(KeyMap.map(special: .function(13), characters: "", modifiers: []) == KeyMap.Output.none)
        #expect(KeyMap.map(special: nil, characters: "é", modifiers: [.control]) == KeyMap.Output.none)
    }
}

struct PaneChoiceTests {
    func pane(_ id: String, _ role: Role, window: String = "@1", shell: Bool = false) -> Pane {
        Pane(id: id, window: window, index: 0, role: role, cols: 80, rows: 24,
             command: shell ? "zsh" : "claude", path: "/", dead: false, shellForeground: shell)
    }

    @Test func taggedRolesToggle() {
        let s = Session(id: "$1", name: "s", panes: [
            pane("%1", .hidden, shell: true), pane("%2", .agent), pane("%3", .shell, shell: true),
        ])
        #expect(s.defaultPane?.id == "%2")
        #expect(s.counterpart(of: "%2")?.id == "%3")
        #expect(s.counterpart(of: "%3")?.id == "%2")
    }

    @Test func untaggedPanesUseTheShellInTheSameWindow() {
        let s = Session(id: "$1", name: "s", panes: [
            pane("%1", .unknown), pane("%2", .unknown, shell: true), pane("%3", .unknown, window: "@2", shell: true),
        ])
        #expect(s.counterpart(of: "%1")?.id == "%2")
        #expect(s.counterpart(of: "%2")?.id == "%1")
    }

    @Test func noCounterpartDisablesTheToggle() {
        let s = Session(id: "$1", name: "s", panes: [pane("%1", .agent)])
        #expect(s.counterpart(of: "%1") == nil)
    }
}

struct RepoGroupTests {
    func session(_ name: String, repo: String?) -> Session {
        Session(id: "$\(name)", name: name, repo: repo, panes: [])
    }

    @Test func groupsByRepoInFirstSeenOrderWithUntaggedLast() {
        let sessions = [
            session("a1", repo: "app"), session("x", repo: nil), session("b1", repo: "api"),
            session("a2", repo: "app"), session("y", repo: ""), session("b2", repo: "api"),
        ]
        let groups = sessions.groupedByRepo()
        #expect(groups.map(\.repo) == ["app", "api", nil])
        #expect(groups.map { $0.sessions.map(\.name) } == [["a1", "a2"], ["b1", "b2"], ["x", "y"]])
    }
}
