package tech.xdge.muxa.protocol

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.serializer
import kotlin.reflect.KClass

/** Encodes and decodes protocol lines: one JSON object, no trailing newline. */
object Wire {
    private val json = Json {
        // PROTOCOL.md: unknown fields must be ignored.
        ignoreUnknownKeys = true
        // Absent optionals and false flags stay off the wire, as the bridge writes them.
        explicitNulls = false
        encodeDefaults = false
    }

    private class Kind<T : Any>(val type: String, val cls: KClass<T>, val serializer: KSerializer<T>)

    private inline fun <reified T : Any> kind(type: String) = Kind(type, T::class, serializer<T>())

    private val messages = listOf(
        kind<Message.Hello>("hello"),
        kind<Message.Result>("result"),
        kind<Message.Screen>("screen"),
        kind<Message.Output>("output"),
        kind<Message.History>("history"),
        kind<Message.SessionsChanged>("sessions-changed"),
        kind<Message.PaneGone>("pane-gone"),
        kind<Message.ChatEntries>("chat-entries"),
        kind<Message.ChatEnded>("chat-ended"),
        kind<Message.DocList>("doc-list"),
        kind<Message.DocContent>("doc-content"),
        kind<Message.DocEnded>("doc-ended"),
        kind<Message.ChangeList>("change-list"),
        kind<Message.DiffContent>("diff-content"),
        kind<Message.DiffEnded>("diff-ended"),
        kind<Message.ImageList>("image-list"),
        kind<Message.ImageContent>("image-content"),
        kind<Message.ImageEnded>("image-ended"),
        kind<Message.TaskCreated>("task-created"),
        kind<Message.TaskList>("task-list"),
        kind<Message.TaskSaved>("task-saved"),
        kind<Message.TasksChanged>("tasks-changed"),
        kind<Message.RoutineList>("routine-list"),
        kind<Message.RoutineSaved>("routine-saved"),
        kind<Message.InboxList>("inbox-list"),
        kind<Message.InboxSaved>("inbox-saved"),
        kind<Message.SessionStarted>("session-started"),
        kind<Message.CaptureSaved>("capture-saved"),
        kind<Message.WorkSaved>("work-saved"),
        kind<Message.WorkDetail>("work-detail"),
        kind<Message.PlanSaved>("plan-saved"),
        kind<Message.Bye>("bye"),
    )

    private val requests = listOf(
        kind<Request.Ping>("ping"),
        kind<Request.List>("list"),
        kind<Request.Watch>("watch"),
        kind<Request.Unwatch>("unwatch"),
        kind<Request.History>("history"),
        kind<Request.Viewport>("viewport"),
        kind<Request.Keys>("keys"),
        kind<Request.Text>("text"),
        kind<Request.RawBytes>("bytes"),
        kind<Request.Paste>("paste"),
        kind<Request.Chat>("chat"),
        kind<Request.Unchat>("unchat"),
        kind<Request.Docs>("docs"),
        kind<Request.Doc>("doc"),
        kind<Request.Undoc>("undoc"),
        kind<Request.Changes>("changes"),
        kind<Request.Diff>("diff"),
        kind<Request.Undiff>("undiff"),
        kind<Request.Images>("images"),
        kind<Request.Image>("image"),
        kind<Request.Unimage>("unimage"),
        kind<Request.NewTask>("new-task"),
        kind<Request.CloseTask>("close-task"),
        kind<Request.ListTasks>("list-tasks"),
        kind<Request.CreateTask>("create-task"),
        kind<Request.UpdateTask>("update-task"),
        kind<Request.RestartAgent>("restart-agent"),
        kind<Request.RecoverRestart>("recover-restart"),
        kind<Request.StartLoop>("start-loop"),
        kind<Request.StopLoop>("stop-loop"),
        kind<Request.ListRoutines>("list-routines"),
        kind<Request.RunRoutine>("run-routine"),
        kind<Request.ListInbox>("list-inbox"),
        kind<Request.UpdateInbox>("update-inbox"),
        kind<Request.StartSession>("start-session"),
        kind<Request.Capture>("capture"),
        kind<Request.SortCapture>("sort-capture"),
        kind<Request.UpdateWork>("update-work"),
        kind<Request.RequestAssistant>("request-assistant"),
        kind<Request.CancelRequest>("cancel-request"),
        kind<Request.GetWork>("get-work"),
        kind<Request.ApplyNotes>("apply-notes"),
        kind<Request.PlanNow>("plan-now"),
        kind<Request.Bye>("bye"),
    )

    fun encode(request: Request): String = encode(request, requests)

    fun encode(message: Message): String =
        if (message is Message.Unknown) {
            JsonObject(mapOf("type" to JsonPrimitive(message.type))).toString()
        } else {
            encode(message, messages)
        }

    /** A type this client does not know decodes as [Message.Unknown]. */
    fun decodeMessage(line: String): Message {
        val (type, body) = split(line)
        val kind = messages.firstOrNull { it.type == type } ?: return Message.Unknown(type)
        return json.decodeFromJsonElement(kind.serializer, body)
    }

    fun decodeRequest(line: String): Request {
        val (type, body) = split(line)
        val kind = requests.firstOrNull { it.type == type }
            ?: throw SerializationException("unknown request type $type")
        return json.decodeFromJsonElement(kind.serializer, body)
    }

    private fun split(line: String): Pair<String, JsonObject> {
        val obj = json.parseToJsonElement(line).jsonObject
        val type = obj["type"]?.jsonPrimitive?.content ?: throw SerializationException("no type")
        return type to JsonObject(obj - "type")
    }

    private fun <T : Any> encode(value: T, kinds: List<Kind<out T>>): String {
        @Suppress("UNCHECKED_CAST")
        val kind = kinds.first { it.cls.isInstance(value) } as Kind<T>
        val body = json.encodeToJsonElement(kind.serializer, value).jsonObject
        return JsonObject(mapOf("type" to JsonPrimitive(kind.type)) + body).toString()
    }
}
