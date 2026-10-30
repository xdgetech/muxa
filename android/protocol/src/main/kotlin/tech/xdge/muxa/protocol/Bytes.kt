package tech.xdge.muxa.protocol

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import java.util.Base64

/** Raw bytes, standard base64 on the wire. Pane content is not UTF-8 text. */
@Serializable(with = BytesSerializer::class)
class Bytes(val array: ByteArray) {
    val size: Int get() = array.size

    override fun equals(other: Any?) = other is Bytes && array.contentEquals(other.array)
    override fun hashCode() = array.contentHashCode()
    // Never the content: pane bytes stay out of logs (AGENTS.md, invariant 5).
    override fun toString() = "Bytes(${array.size})"
}

internal object BytesSerializer : KSerializer<Bytes> {
    override val descriptor = PrimitiveSerialDescriptor("Bytes", PrimitiveKind.STRING)
    override fun serialize(encoder: Encoder, value: Bytes) =
        encoder.encodeString(Base64.getEncoder().encodeToString(value.array))

    override fun deserialize(decoder: Decoder): Bytes =
        try {
            Bytes(Base64.getDecoder().decode(decoder.decodeString()))
        } catch (e: IllegalArgumentException) {
            throw SerializationException("invalid base64", e)
        }
}
