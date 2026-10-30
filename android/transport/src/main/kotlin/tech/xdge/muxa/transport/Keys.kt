package tech.xdge.muxa.transport

import kotlinx.serialization.Serializable
import org.bouncycastle.crypto.params.Ed25519PrivateKeyParameters
import org.bouncycastle.crypto.util.PrivateKeyInfoFactory
import org.bouncycastle.crypto.util.SubjectPublicKeyInfoFactory
import org.bouncycastle.jce.provider.BouncyCastleProvider
import java.io.ByteArrayOutputStream
import java.io.DataOutputStream
import java.nio.ByteBuffer
import java.security.KeyFactory
import java.security.KeyPair
import java.security.MessageDigest
import java.security.SecureRandom
import java.security.spec.PKCS8EncodedKeySpec
import java.security.spec.X509EncodedKeySpec
import java.util.Base64

/** An SSH public key in OpenSSH text form (`ssh-ed25519 AAAA…`), without a comment. */
@Serializable
data class HostKey(val openSSH: String) {
    /** `SHA256:…`, as `ssh-keygen -lf` prints it. */
    val fingerprint: String
        get() {
            val blob = Base64.getDecoder().decode(openSSH.split(" ")[1])
            val digest = MessageDigest.getInstance("SHA-256").digest(blob)
            return "SHA256:" + Base64.getEncoder().withoutPadding().encodeToString(digest)
        }

    companion object {
        /** Checks the text and drops a comment. Throws [TransportError.Failed] when it is no key. */
        fun parse(text: String): HostKey {
            val parts = text.trim().split(Regex("\\s+"))
            val blob = parts.getOrNull(1)?.let { runCatching { Base64.getDecoder().decode(it) }.getOrNull() }
            if (blob == null || blobType(blob) != parts[0]) throw TransportError.Failed("not an SSH public key")
            return HostKey("${parts[0]} ${parts[1]}")
        }

        fun of(type: String, blob: ByteArray) = HostKey("$type ${Base64.getEncoder().encodeToString(blob)}")

        /** The key type named at the start of an SSH public key blob. */
        private fun blobType(blob: ByteArray): String? = runCatching {
            val b = ByteBuffer.wrap(blob)
            val n = b.getInt()
            if (n < 0 || n > b.remaining()) return null
            String(blob, 4, n, Charsets.US_ASCII)
        }.getOrNull()
    }
}

/** The device's SSH identity: one ed25519 key. */
class ClientKey private constructor(private val params: Ed25519PrivateKeyParameters) {
    /** The 32-byte private key, as stored. */
    val raw: ByteArray get() = params.encoded

    /** The key as `java.security` objects, from BouncyCastle (sshj signs with them). */
    fun keyPair(): KeyPair {
        val factory = KeyFactory.getInstance("Ed25519", PROVIDER)
        val private = factory.generatePrivate(PKCS8EncodedKeySpec(PrivateKeyInfoFactory.createPrivateKeyInfo(params).encoded))
        val public = factory.generatePublic(
            X509EncodedKeySpec(SubjectPublicKeyInfoFactory.createSubjectPublicKeyInfo(params.generatePublicKey()).encoded),
        )
        return KeyPair(public, private)
    }

    val publicKey: HostKey
        get() {
            val blob = ByteArrayOutputStream()
            DataOutputStream(blob).use { out ->
                val type = TYPE.toByteArray(Charsets.US_ASCII)
                out.writeInt(type.size)
                out.write(type)
                val key = params.generatePublicKey().encoded
                out.writeInt(key.size)
                out.write(key)
            }
            return HostKey.of(TYPE, blob.toByteArray())
        }

    /** The line to add to `~/.ssh/authorized_keys` (see SECURITY.md for the restrictions). */
    fun authorizedKey(comment: String): String = "${publicKey.openSSH} $comment"

    companion object {
        const val TYPE = "ssh-ed25519"
        private val PROVIDER = BouncyCastleProvider()

        fun generate() = ClientKey(Ed25519PrivateKeyParameters(SecureRandom()))

        fun fromRaw(raw: ByteArray): ClientKey {
            if (raw.size != Ed25519PrivateKeyParameters.KEY_SIZE) throw TransportError.Failed("the stored key is damaged")
            return ClientKey(Ed25519PrivateKeyParameters(raw, 0))
        }
    }
}
