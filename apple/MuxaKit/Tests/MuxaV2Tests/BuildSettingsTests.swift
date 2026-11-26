import Foundation
import Testing

@testable import MuxaV2

/// The build disk's settings and op on the wire, and in the demo (ADR 0102).
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
            (.buildUpdate(extraPaths: ["out", "dist"]), #"{"extra_paths":["out","dist"],"op":"build-update"}"#),
            (.buildUpdate(extraPaths: []), #"{"extra_paths":[],"op":"build-update"}"#),
        ]
        for (op, json) in cases {
            #expect(try wire(op) == json, "\(op)")
            #expect(try Coding.plainDecoder.decode(SettingsOp.self, from: Data(json.utf8)) == op)
        }
    }

    @Test func theViewCarriesTheBuildDisk() throws {
        let wire = #"{"revision":"1","build":{"dir":"/ssd/build","drop_after":null,"dir_found":false,"extra_paths":["out"]}}"#
        let settings = try Coding.plainDecoder.decode(Settings.self, from: Data(wire.utf8))
        #expect(settings.build == SettingsBuild(dir: "/ssd/build", dropAfter: nil, dirFound: false, extraPaths: ["out"]))
    }

    @Test func anOlderServerSendsNoBuildDiskOrExtraPaths() throws {
        let none = try Coding.plainDecoder.decode(Settings.self, from: Data(#"{"revision":"1"}"#.utf8))
        #expect(none.build == nil)
        let wire = #"{"revision":"1","build":{"dir":"/ssd/build","dir_found":true}}"#
        let settings = try Coding.plainDecoder.decode(Settings.self, from: Data(wire.utf8))
        #expect(settings.build?.extraPaths == [])
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemoAppliesTheBuildDisk() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let settings = try await c.settings()
        let build = try #require(settings.build)
        #expect(build.dir != nil && build.dirFound && build.extraPaths == ["out"])
        let after = try await c.settingsUpdate(revision: settings.revision, ops: [
            .buildUpdate(dropAfter: .set("12h"), extraPaths: ["out", "dist"]),
        ])
        #expect(after.build == SettingsBuild(dir: build.dir, dropAfter: "12h", dirFound: true, extraPaths: ["out", "dist"]))
        let off = try await c.settingsUpdate(revision: after.revision, ops: [.buildUpdate(dir: .clear)])
        #expect(off.build == SettingsBuild(dir: nil, dropAfter: "12h", dirFound: false, extraPaths: ["out", "dist"]))
        let cleared = try await c.settingsUpdate(revision: off.revision, ops: [.buildUpdate(extraPaths: [])])
        #expect(cleared.build?.extraPaths == [])
    }
}
