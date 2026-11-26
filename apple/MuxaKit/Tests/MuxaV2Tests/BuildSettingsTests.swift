import Foundation
import Testing

@testable import MuxaV2

/// The build disk's settings and ops on the wire, and in the demo (ADR 0102).
struct BuildSettingsTests {
    func wire(_ op: SettingsOp) throws -> String {
        String(decoding: try Coding.plainEncoder.encode(op), as: UTF8.self)
    }

    @Test func buildUpdateKeepsAnAbsentFieldAndNullClearsIt() throws {
        let cases: [(SettingsOp, String)] = [
            (.buildUpdate(), #"{"op":"build-update"}"#),
            (.buildUpdate(dir: .clear), #"{"dir":null,"op":"build-update"}"#),
            (.buildUpdate(dir: .set("~/build"), dropAfter: .set("12h")),
             #"{"dir":"~\/build","drop_after":"12h","op":"build-update"}"#),
            (.buildUpdate(dropAfter: .clear), #"{"drop_after":null,"op":"build-update"}"#),
            (.workspaceUpdate(workspace: "w", buildPaths: ["target", "dist"]),
             #"{"build_paths":["target","dist"],"op":"workspace-update","workspace":"w"}"#),
        ]
        for (op, json) in cases {
            #expect(try wire(op) == json, "\(op)")
            #expect(try Coding.plainDecoder.decode(SettingsOp.self, from: Data(json.utf8)) == op)
        }
    }

    @Test func theViewCarriesTheBuildDiskAndEachWorkspacesPaths() throws {
        let wire = #"""
            {"revision":"1","build":{"dir":"/ssd/build","drop_after":null,"dir_found":false},
             "workspaces":[{"name":"w","root":"/w","build_paths":["target","dist"]}]}
            """#
        let settings = try Coding.plainDecoder.decode(Settings.self, from: Data(wire.utf8))
        #expect(settings.build == SettingsBuild(dir: "/ssd/build", dropAfter: nil, dirFound: false))
        #expect(settings.workspaces[0].buildPaths == ["target", "dist"])
    }

    @Test func anOlderServerSendsNoBuildDisk() throws {
        let wire = #"{"revision":"1","workspaces":[{"name":"w","root":"/w"}]}"#
        let settings = try Coding.plainDecoder.decode(Settings.self, from: Data(wire.utf8))
        #expect(settings.build == nil)
        #expect(settings.workspaces[0].buildPaths.isEmpty)
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemoAppliesTheBuildDiskAndPaths() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let settings = try await c.settings()
        let build = try #require(settings.build)
        #expect(build.dir != nil && build.dirFound)
        let after = try await c.settingsUpdate(revision: settings.revision, ops: [
            .buildUpdate(dropAfter: .set("12h")),
            .workspaceUpdate(workspace: "app", buildPaths: ["target"]),
        ])
        #expect(after.build == SettingsBuild(dir: build.dir, dropAfter: "12h", dirFound: true))
        #expect(after.workspaces.first { $0.name == "app" }?.buildPaths == ["target"])
        let off = try await c.settingsUpdate(revision: after.revision, ops: [.buildUpdate(dir: .clear)])
        #expect(off.build == SettingsBuild(dir: nil, dropAfter: "12h", dirFound: false))
    }
}
