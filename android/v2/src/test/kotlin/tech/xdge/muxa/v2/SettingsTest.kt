package tech.xdge.muxa.v2

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/** Settings (ADR 0074): the ops' wire shape and lenient decoding. */
class SettingsTest {
    @Test
    fun opsEncodeToTheProtocolsJson() {
        val ops = listOf(
            SettingsOp.WorkspaceUpdate("w") to """{"op":"workspace-update","workspace":"w"}""",
            SettingsOp.WorkspaceUpdate("w", agentCommand = Patch.Clear, parkAfter = Patch.Set("2d")) to
                """{"op":"workspace-update","workspace":"w","agent_command":null,"park_after":"2d"}""",
            SettingsOp.WorkspaceAdd("w", "~/w") to """{"op":"workspace-add","name":"w","root":"~/w","repos":[]}""",
            SettingsOp.EnvSet("w", "FOO", "bar") to """{"op":"env-set","workspace":"w","key":"FOO","value":"bar"}""",
            SettingsOp.SetLoopChecks("w", null) to """{"op":"loop-checks","workspace":"w","checks":null}""",
            SettingsOp.SetLoopChecks("w", LoopChecks.Any) to """{"op":"loop-checks","workspace":"w","checks":"any"}""",
            SettingsOp.RoutinesGate("w", true) to """{"op":"routines-gate","workspace":"w","enabled":true}""",
            SettingsOp.ConnectionAdd("w", ConnectionSpec("gh", "GH_TOKEN", access = "read"), "s3cret") to
                """{"op":"connection-add","workspace":"w","connection":{"name":"gh","env":"GH_TOKEN","access":"read"},"secret":"s3cret"}""",
            SettingsOp.ConnectionSecret("w", "gh", "x") to """{"op":"connection-secret","workspace":"w","name":"gh","secret":"x"}""",
            SettingsOp.McpAdd("w", McpSpec("api", url = "https://api.example.com/mcp", type = "sse")) to
                """{"op":"mcp-add","workspace":"w","mcp":{"name":"api","url":"https://api.example.com/mcp","type":"sse","env":{},"headers":{},"connections":[]}}""",
            SettingsOp.RoutineAdd("w", RoutineSpec("d", at = "08:00", instructions = "routines/d.md"), "Hi.\n") to
                """{"op":"routine-add","workspace":"w","routine":{"name":"d","at":"08:00","instructions":"routines/d.md","enabled":true,"direct_writes":false,"read_only":false},"instructions_text":"Hi.\n"}""",
            SettingsOp.BotAdd("w", BotSpec("pr-watch", description = "Watches PRs"), "I watch PRs.\n") to
                """{"op":"bot-add","workspace":"w","bot":{"name":"pr-watch","description":"Watches PRs","enabled":true},"soul_text":"I watch PRs.\n"}""",
            SettingsOp.BotMemoryRestore("w", "pr-watch", 1) to
                """{"op":"bot-memory-restore","workspace":"w","name":"pr-watch","version":1}""",
            SettingsOp.WorkerAdd(WorkerSpec("r", agent = "codex", loop = WorkerLoop("make test", maxAttempts = 2))) to
                """{"op":"worker-add","worker":{"name":"r","agent":"codex","args":[],"loop":{"check":"make test","max_attempts":2}}}""",
            SettingsOp.WorkerRemove("r") to """{"op":"worker-remove","name":"r"}""",
            SettingsOp.BotUpdate("w", BotSpec("lead", members = listOf("a", "b"), handoffsPerDay = 5)) to
                """{"op":"bot-update","workspace":"w","bot":{"name":"lead","enabled":true,"members":["a","b"],"handoffs_per_day":5}}""",
            SettingsOp.RoutineAdd("w", RoutineSpec("r", instructions = "routines/r.md", on = "review")) to
                """{"op":"routine-add","workspace":"w","routine":{"name":"r","instructions":"routines/r.md","enabled":true,"direct_writes":false,"read_only":false,"on":"review"}}""",
            SettingsOp.WorkspaceUpdate("w", claudeConnectors = true) to
                """{"op":"workspace-update","workspace":"w","claude_connectors":true}""",
            SettingsOp.WorkspaceUpdate("w", botRules = BotRules(deny = listOf("Bash"))) to
                """{"op":"workspace-update","workspace":"w","bot_rules":{"allow":[],"deny":["Bash"]}}""",
            SettingsOp.AssistantUpdate(true) to """{"op":"assistant-update","enabled":true}""",
            SettingsOp.AssistantUpdate(false, "reviewer", "Be brief.") to
                """{"op":"assistant-update","enabled":false,"worker":"reviewer","soul_text":"Be brief."}""",
        )
        for ((op, expected) in ops) {
            assertEquals(json.parseToJsonElement(expected), op.json())
            assertEquals(op, SettingsOp.fromJson(op.json()))
        }
        val request = V2Request.SettingsUpdate("1", listOf(SettingsOp.WorkspaceRemove("w"))).json(7)
        assertEquals(
            json.parseToJsonElement("""{"id":7,"type":"settings-update","revision":"1","ops":[{"op":"workspace-remove","workspace":"w"}]}"""),
            request,
        )
        assertEquals(json.parseToJsonElement("""{"id":8,"type":"settings"}"""), V2Request.GetSettings.json(8))
    }

    @Test
    fun botRequestsEncodeToTheProtocolsJson() {
        assertEquals(
            json.parseToJsonElement("""{"id":1,"type":"start-task","workspace":"app","title":"Talk to pr-watch","places":[],"plan":false,"bot":"pr-watch"}"""),
            V2Request.StartTask("app", "Talk to pr-watch", bot = "pr-watch").json(1),
        )
        assertEquals(json.parseToJsonElement("""{"id":2,"type":"inbox-apply","item":"d12"}"""), V2Request.InboxApply("d12").json(2))
        assertEquals(
            json.parseToJsonElement("""{"id":3,"type":"inbox-suggestion","item":"d13","accept":false}"""),
            V2Request.InboxSuggestion("d13", accept = false).json(3),
        )
    }

    @Test
    fun anItemsProposalAndSuggestionDecodeAndAnUnknownOpIsLeftOut() {
        val item = json.decodeFromString<InboxItem>(
            """{"id":"i","kind":"decide","title":"t","created":1,"source":"task",
               "settings_proposal":{"revision":"r7","ops":[{"op":"bot-update","workspace":"w","bot":{"name":"b","enabled":true}},{"op":"new-op","x":1}]},
               "suggestion":{"kind":"reply","split":["a","b"],"task_goal":"Do it","bot":"b"}}""",
        )
        assertEquals(SettingsProposal("r7", listOf(SettingsOp.BotUpdate("w", BotSpec("b")))), item.settingsProposal)
        assertEquals(ItemSuggestion(kind = ItemKind.REPLY, split = listOf("a", "b"), taskGoal = "Do it", bot = "b"), item.suggestion)
        val old = json.decodeFromString<InboxItem>("""{"id":"i","kind":"todo","title":"t","created":1,"source":"capture"}""")
        assertEquals(null to null, old.settingsProposal to old.suggestion)
        val r = json.decodeFromString<Routine>("""{"workspace":"w","name":"r","enabled":true,"on":"review"}""")
        assertEquals("review", r.on)
    }

    @Test
    fun writeOnlyValuesNeverShowInToString() {
        val values = listOf(
            SettingsOp.EnvSet("w", "FOO", "bar-value"),
            SettingsOp.ConnectionAdd("w", ConnectionSpec("gh", "GH"), "s3cret"),
            SettingsOp.ConnectionUpdate("w", ConnectionSpec("gh", "GH"), "s3cret"),
            SettingsOp.ConnectionSecret("w", "gh", "s3cret"),
            V2Request.SettingsUpdate("1", listOf(SettingsOp.ConnectionSecret("w", "gh", "s3cret"))),
        )
        for (v in values) {
            val text = v.toString()
            assertFalse(text, "s3cret" in text || "bar-value" in text)
        }
    }

    @Test
    fun settingsDecodeLeniently() {
        val s = json.decodeFromString<Settings>(
            """{"revision":"r","new":1,"workspaces":[{"name":"w","root":"~/w","connections":[{"name":"gh","env":"GH","secret":"sealed"}],"routines":[{"name":"d","instructions":"d.md","enabled":false}]}]}""",
        )
        val w = s.workspaces.single()
        assertEquals(SettingsAgent(), w.agent)
        assertEquals(null, w.loop)
        assertEquals(SecretState.UNKNOWN, w.connections.single().secret)
        assertEquals(null, w.routines.single().instructionsText)
        assertEquals(emptyList<WorkerSpec>(), s.workers)
        assertEquals(null, s.assistant)
        assertEquals(false, w.claudeConnectors)
        val on = json.decodeFromString<Settings>("""{"revision":"r","workspaces":[{"name":"w","root":"~/w","claude_connectors":true}]}""")
        assertEquals(true, on.workspaces.single().claudeConnectors)
        assertEquals(BotRules(), w.botRules)
        val rules = json.decodeFromString<Settings>("""{"revision":"r","workspaces":[{"name":"w","root":"~/w","bot_rules":{"allow":["WebSearch"]}}]}""")
        assertEquals(BotRules(allow = listOf("WebSearch")), rules.workspaces.single().botRules)
        val counts = json.decodeFromString<Inbox>(
            """{"revision":1,"items":[],"done":[],"bots":[{"workspace":"w","bot":"b","done":1,"deleted":0,"day":"2026-11-23","added":2},{"workspace":"w","bot":"old","done":0,"deleted":0}]}""",
        ).bots
        assertEquals(listOf(BotCount("w", "b", 1, 0, "2026-11-23", 2), BotCount("w", "old", 0, 0)), counts)
        val withAssistant = json.decodeFromString<Settings>(
            """{"revision":"r","assistant":{"enabled":true,"soul_text":"Be brief.","memory_text":null}}""",
        )
        assertEquals(SettingsAssistant(true, soulText = "Be brief."), withAssistant.assistant)
    }
}
