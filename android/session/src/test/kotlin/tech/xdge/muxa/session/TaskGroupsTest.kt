package tech.xdge.muxa.session

import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Test
import tech.xdge.muxa.protocol.Session
import tech.xdge.muxa.protocol.TaskRecord
import tech.xdge.muxa.protocol.TaskStatus
import tech.xdge.muxa.protocol.WorkspaceInfo
import java.io.File

class TaskGroupsTest {
    private data class Repo(val repo: String?, val label: String, val heading: Boolean, val tasks: List<String>)
    private data class Group(val workspace: String, val repos: List<Repo>)

    /** `protocol/fixtures/tasks/grouping.jsonl` pins the rule shared with `TaskGroups.swift`. */
    @Test
    fun groupsMatchTheSharedFixture() {
        val json = Json { ignoreUnknownKeys = true }
        val lines = File(System.getProperty("muxa.fixtures")).resolve("tasks/grouping.jsonl").readLines()
            .filter { it.isNotEmpty() }
        assertEquals(4, lines.size)
        for (line in lines) {
            val case = json.parseToJsonElement(line).jsonObject
            val workspaces = json.decodeFromJsonElement(ListSerializer(WorkspaceInfo.serializer()), case.getValue("workspaces"))
            val records = json.decodeFromJsonElement(ListSerializer(TaskRecord.serializer()), case.getValue("records"))
            val sessions = json.decodeFromJsonElement(ListSerializer(Session.serializer()), case.getValue("sessions"))
            val now = case["now"]?.jsonPrimitive?.long
            val want = case.getValue("groups").jsonArray.map { g ->
                val o = g.jsonObject
                Group(o.getValue("workspace").jsonPrimitive.content, o.getValue("repos").jsonArray.map { r ->
                    val ro = r.jsonObject
                    Repo(
                        ro.getValue("repo").takeIf { it != JsonNull }?.jsonPrimitive?.content,
                        ro.getValue("label").jsonPrimitive.content,
                        ro.getValue("heading").jsonPrimitive.boolean,
                        ro.getValue("tasks").jsonArray.map { it.jsonPrimitive.content },
                    )
                })
            }
            val got = inbox(records, sessions, now).groupedByWorkspace(workspaces).map { w ->
                Group(w.workspace, w.repos.map { Repo(it.repo, it.label, it.heading, it.items.map { i -> i.record.id }) })
            }
            assertEquals(case.getValue("name").jsonPrimitive.content, want, got)
        }
    }

    @Test
    fun foldKeysDependOnlyOnWorkspaceAndRepo() {
        val early = listOf(TaskRecord("a", 1, "work", "a", "g", TaskStatus.OPEN, 1, 1, repo = "/src/app"))
        val later = listOf(early[0].copy(updatedAt = 9, status = TaskStatus.DONE))
        fun keys(records: List<TaskRecord>) =
            inbox(records, emptyList()).groupedByWorkspace(emptyList()).flatMap { w -> listOf(w.foldKey) + w.repos.map { it.foldKey } }
        assertEquals(listOf("w:work", "r:work:/src/app"), keys(early))
        assertEquals(keys(early), keys(later))
    }
}
