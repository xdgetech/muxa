import Foundation
import Testing

@testable import MuxaV2

/// The `settings-update` ops on the wire, and the settings view's decoding.
struct SettingsOpTests {
    func wire(_ op: SettingsOp) throws -> String {
        String(decoding: try Coding.plainEncoder.encode(op), as: UTF8.self)
    }

    @Test func workspaceUpdateSendsNullToClearAndNothingToKeep() throws {
        #expect(try wire(.workspaceUpdate(workspace: "w")) == #"{"op":"workspace-update","workspace":"w"}"#)
        #expect(try wire(.workspaceUpdate(workspace: "w", agentCommand: .clear, parkAfter: .set("2d")))
            == #"{"agent_command":null,"op":"workspace-update","park_after":"2d","workspace":"w"}"#)
        #expect(try wire(.workspaceUpdate(workspace: "w", root: "~/w", repos: ["a"], agentCommand: .set("claude")))
            == #"{"agent_command":"claude","op":"workspace-update","repos":["a"],"root":"~\/w","workspace":"w"}"#)
    }

    @Test func eachOpHasTheProtocolsKeys() throws {
        let cases: [(SettingsOp, String)] = [
            (.workspaceAdd(name: "w", root: "/w"), #"{"name":"w","op":"workspace-add","repos":[],"root":"\/w"}"#),
            (.workspaceRemove(workspace: "w"), #"{"op":"workspace-remove","workspace":"w"}"#),
            (.envSet(workspace: "w", key: "K", value: "v"), #"{"key":"K","op":"env-set","value":"v","workspace":"w"}"#),
            (.envUnset(workspace: "w", key: "K"), #"{"key":"K","op":"env-unset","workspace":"w"}"#),
            (.loopChecks(workspace: "w", checks: nil), #"{"checks":null,"op":"loop-checks","workspace":"w"}"#),
            (.loopChecks(workspace: "w", checks: .any), #"{"checks":"any","op":"loop-checks","workspace":"w"}"#),
            (.routinesGate(workspace: "w", enabled: true),
             #"{"direct_writes":false,"enabled":true,"op":"routines-gate","workspace":"w"}"#),
            (.connectionAdd(workspace: "w", connection: ConnectionSpec(name: "gh", env: "GH_TOKEN", access: "read"), secret: "s"),
             #"{"connection":{"access":"read","env":"GH_TOKEN","name":"gh"},"op":"connection-add","secret":"s","workspace":"w"}"#),
            (.connectionUpdate(workspace: "w", connection: ConnectionSpec(name: "gh", env: "GH_TOKEN")),
             #"{"connection":{"env":"GH_TOKEN","name":"gh"},"op":"connection-update","workspace":"w"}"#),
            (.connectionSecret(workspace: "w", name: "gh", secret: "s"),
             #"{"name":"gh","op":"connection-secret","secret":"s","workspace":"w"}"#),
            (.connectionRemove(workspace: "w", name: "gh"), #"{"name":"gh","op":"connection-remove","workspace":"w"}"#),
            (.mcpAdd(workspace: "w", mcp: McpSpec(name: "d", url: "https://d", transport: "sse")),
             #"{"mcp":{"connections":[],"env":{},"headers":{},"name":"d","type":"sse","url":"https:\/\/d"},"op":"mcp-add","workspace":"w"}"#),
            (.mcpRemove(workspace: "w", name: "d"), #"{"name":"d","op":"mcp-remove","workspace":"w"}"#),
            (.routineAdd(workspace: "w", routine: RoutineSpec(name: "r", at: "08:00", instructions: "r.md"), instructionsText: "Do.\n"),
             #"{"instructions_text":"Do.\n","op":"routine-add","routine":{"at":"08:00","direct_writes":false,"enabled":true,"instructions":"r.md","name":"r","read_only":false},"workspace":"w"}"#),
            (.botAdd(workspace: "w", bot: BotSpec(name: "pr-watch", description: "Watches PRs"), soulText: "I watch PRs.\n"),
             #"{"bot":{"description":"Watches PRs","enabled":true,"name":"pr-watch"},"op":"bot-add","soul_text":"I watch PRs.\n","workspace":"w"}"#),
            (.botRemove(workspace: "w", name: "pr-watch"), #"{"name":"pr-watch","op":"bot-remove","workspace":"w"}"#),
            (.botMemoryRestore(workspace: "w", name: "pr-watch", version: 1),
             #"{"name":"pr-watch","op":"bot-memory-restore","version":1,"workspace":"w"}"#),
            (.routineRemove(workspace: "w", name: "r"), #"{"name":"r","op":"routine-remove","workspace":"w"}"#),
            (.workerAdd(worker: WorkerSpec(name: "x", agent: "codex", loop: WorkerLoop(check: "make test", maxAttempts: 2))),
             #"{"op":"worker-add","worker":{"agent":"codex","args":[],"loop":{"check":"make test","max_attempts":2},"name":"x"}}"#),
            (.workerRemove(name: "x"), #"{"name":"x","op":"worker-remove"}"#),
            (.subscriptionAdd(subscription: SubscriptionSpec(name: "work", agent: "claude-code", home: "~/.claude-work",
                                                             plan: "Max 20x")),
             #"{"op":"subscription-add","subscription":{"agent":"claude-code","home":"~\/.claude-work","name":"work","plan":"Max 20x"}}"#),
            (.subscriptionRemove(name: "work"), #"{"name":"work","op":"subscription-remove"}"#),
            (.subscriptionAdopt(workspace: "w", agent: "claude-code", name: "work", plan: "Max 20x"),
             #"{"agent":"claude-code","name":"work","op":"subscription-adopt","plan":"Max 20x","workspace":"w"}"#),
            (.workspaceUpdate(workspace: "w", subscriptions: ["work", "spare"]),
             #"{"op":"workspace-update","subscriptions":["work","spare"],"workspace":"w"}"#),
            (.assistantUpdate(enabled: true, subscriptions: ["work"]),
             #"{"enabled":true,"op":"assistant-update","subscriptions":["work"]}"#),
            (.botUpdate(workspace: "w", bot: BotSpec(name: "b", subscriptions: ["spare"])),
             #"{"bot":{"enabled":true,"name":"b","subscriptions":["spare"]},"op":"bot-update","workspace":"w"}"#),
        ]
        for (op, json) in cases {
            #expect(try wire(op) == json, "\(op.name)")
            // The demo decodes what the client sends.
            #expect(try Coding.plainDecoder.decode(SettingsOp.self, from: Data(json.utf8)) == op)
        }
    }

    @Test func aRequestCarriesTheOps() throws {
        let data = try V2Request.settingsUpdate(revision: "r1", ops: [.workspaceUpdate(workspace: "w", parkAfter: .clear)])
            .json(id: 7)
        #expect(String(decoding: data, as: UTF8.self)
            == #"{"id":7,"ops":[{"op":"workspace-update","park_after":null,"workspace":"w"}],"revision":"r1","type":"settings-update"}"#)
    }

    @Test func secretsAndValuesNeverShowInDescriptions() {
        let ops: [SettingsOp] = [
            .envSet(workspace: "w", key: "K", value: "v4lue"),
            .connectionAdd(workspace: "w", connection: ConnectionSpec(name: "gh", env: "GH"), secret: "s3cret"),
            .connectionUpdate(workspace: "w", connection: ConnectionSpec(name: "gh", env: "GH"), secret: "s3cret"),
            .connectionSecret(workspace: "w", name: "gh", secret: "s3cret"),
        ]
        let request = V2Request.settingsUpdate(revision: "1", ops: ops)
        var dumped = ""
        dump(ops, to: &dumped)
        for shown in ["\(ops)", String(reflecting: ops), String(describing: request), String(reflecting: request), dumped] {
            #expect(!shown.contains("s3cret") && !shown.contains("v4lue"), "\(shown)")
        }
        #expect("\(ops[0])".contains("K=<write-only>"))
    }

    @Test func theViewDecodesLeniently() throws {
        let json = #"""
            {"revision":"abc","workspaces":[{"name":"w","root":"~/w","agent":{"env":[{"key":"MY_VAR","set":true}]},
              "connections":[{"name":"gh","env":"GH_TOKEN","secret":"rotating"}],
              "mcp":[{"name":"d","command":"npx","env":{"API_BASE":"x"}}],
              "routines":[{"name":"r","instructions":"r.md","enabled":false}]}]}
            """#
        let s = try Coding.plainDecoder.decode(Settings.self, from: Data(json.utf8))
        #expect(!s.readOnly && s.problem == nil && s.workers.isEmpty)
        let w = try #require(s.workspaces.first)
        #expect(w.repos.isEmpty && w.loop == nil && w.parkAfter == nil && !w.routinesEnabled)
        #expect(w.agent.env == [EnvKey(key: "MY_VAR")])
        #expect(w.connections[0].secret == .unknown("rotating"))
        #expect(w.mcp[0].env == ["API_BASE": "x"], "variable names keep their case")
        #expect(w.routines[0].spec.enabled == false && w.routines[0].instructionsText == nil)
    }
}
