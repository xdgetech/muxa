package tech.xdge.muxa.protocol

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonEncoder
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive

/** Missing means Keep; explicit JSON null means Workspace default. */
@Serializable(with = WorkerSelectionSerializer::class)
sealed interface WorkerSelection {
    data object Keep : WorkerSelection
    data object WorkspaceDefault : WorkerSelection
    data class Named(val name: String) : WorkerSelection
}

object WorkerSelectionSerializer : KSerializer<WorkerSelection> {
    override val descriptor = PrimitiveSerialDescriptor("WorkerSelection", PrimitiveKind.STRING)
    override fun deserialize(decoder: Decoder): WorkerSelection {
        val value = (decoder as JsonDecoder).decodeJsonElement()
        return when {
            value == JsonNull -> WorkerSelection.WorkspaceDefault
            value is JsonPrimitive && value.isString -> WorkerSelection.Named(value.content)
            else -> throw SerializationException("worker must be a string or null")
        }
    }
    override fun serialize(encoder: Encoder, value: WorkerSelection) {
        val json = when (value) {
            WorkerSelection.Keep -> throw SerializationException("Keep must be omitted by the enclosing request")
            WorkerSelection.WorkspaceDefault -> JsonNull
            is WorkerSelection.Named -> JsonPrimitive(value.name)
        }
        (encoder as JsonEncoder).encodeJsonElement(json)
    }
}

/** Requested run assignment; an unknown kind is never Workspace default. */
@Serializable
data class WorkerAssignment(val kind: String, val name: String? = null) {
    val isKnown: Boolean get() = when (kind) {
        "workspace-default" -> name == null
        "worker" -> !name.isNullOrEmpty()
        else -> false
    }
}

/** Public host metadata; no args, instructions or environment values. */
@Serializable
data class WorkerInfo(
    val name: String,
    val agent: String,
    val description: String? = null,
    val workspaces: List<String>? = null,
    val loop: LoopSpec? = null,
)
