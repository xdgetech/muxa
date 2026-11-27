import Foundation
import Testing

@testable import MuxaV2

/// `host` and `host-watch` (ADR 0105): decoding, and the demo's answer.
struct HostTests {
    @Test
    func aReportDecodesWithUnknownKindsAndPressure() throws {
        let json = #"""
        {"sampled_ms":1795700000000,"cpu":{"percent":12.5,"cores":10},"load":[2.1,2.4,2.6],
         "memory":{"total":25769803776,"used":24696061952,"pressure":"squeezed"},
         "swap":{"used":4402341478,"total":5368709120},
         "disks":[{"name":"build","path":"/Volumes/B","free":1,"total":2},
                  {"name":"system","path":"/","free":44023414784,"total":494384795648}],
         "groups":[{"kind":"task","name":"Host resources","terminal":"t7","workspace":"xdge","cpu":0.3,
                    "memory":429496729,"processes":6,"idle_since_ms":1795690000000},
                   {"kind":"claude-outside","name":"Claude Code","cpu":0.1,"memory":3,"processes":2},
                   {"kind":"robot","name":"R2","cpu":0,"memory":1,"processes":1}],
         "alerts":["memory","disk:/"]}
        """#
        let r = try Coding.decoder.decode(HostReport.self, from: Data(json.utf8))
        #expect(r.sampledMs == 1_795_700_000_000)
        #expect(r.memory.compressed == nil)
        #expect(r.memory.pressure == .unknown("squeezed"))
        #expect(r.groups.map(\.kind) == [.task, .claudeOutside, .unknown("robot")])
        #expect(r.groups[0].idleSinceMs == 1_795_690_000_000)
        #expect(r.groups[0].isAgentWork && !r.groups[1].isAgentWork)
        #expect(r.systemDisk?.path == "/")
        #expect(r.buildDisk?.path == "/Volumes/B")
        #expect(r.memoryAlert && !r.cpuAlert)
        #expect(try r.alerted(#require(r.systemDisk)))
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemoReportsItsAgentsAndSendsOneAtOnceOnWatch() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let report = try await c.host()
        #expect(report.alerts == ["memory"])
        #expect(report.memory.pressure == .warn)
        let work = report.groups.filter(\.isAgentWork)
        #expect(Set(work.compactMap(\.terminal)) == ["t1", "t2", "t5", "t7", "t8", "t9"])
        #expect(work.map(\.memory) == work.map(\.memory).sorted(by: >), "biggest first")
        #expect(work.first { $0.terminal == "t7" }?.idleSinceMs != nil)
        #expect(work.first { $0.terminal == "t1" }?.idleSinceMs == nil)
        #expect(Set(report.groups.map(\.kind)).isSuperset(of: [.simulator, .app, .docker, .claudeOutside, .other]))

        var events = c.events.makeAsyncIterator()
        try await c.hostWatch()
        guard case .host(let sent)? = await events.next() else { return #expect(Bool(false)) }
        #expect(sent.groups.count == report.groups.count)
        try await c.hostUnwatch()
    }

    @Test(.timeLimit(.minutes(1)))
    func theDemoAnswersDiskWithTaskPlacesAndLeftovers() async throws {
        let c = try await V2Client.open(DemoChannel(), name: "test")
        let rows = try await c.disk()
        #expect(rows.filter { $0.kind == "task-worktree" }.map(\.path) == ["/demo/app-fix-login", "/demo/app-docs-audit"])
        #expect(rows.filter(\.cleanable).map(\.kind) == ["claude-worktree", "build-folder"])
    }
}
