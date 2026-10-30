package tech.xdge.muxa.app

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import tech.xdge.muxa.transport.ClientKey
import tech.xdge.muxa.transport.TransportError
import java.io.File
import java.io.IOException
import java.security.GeneralSecurityException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Keeps the device's SSH key in app storage, sealed with AES-GCM under an Android
 * Keystore key that works only while the device is unlocked. The analog of the
 * iOS Keychain's `WhenUnlockedThisDeviceOnly`. Reconnects happen in the foreground.
 */
class ClientKeyStore(context: Context) {
    private val file = File(context.noBackupFilesDir, "client-key")

    /** The stored key, or a new one that is stored first. */
    fun loadOrCreate(): ClientKey = try {
        if (file.exists()) ClientKey.fromRaw(open(file.readBytes())) else ClientKey.generate().also { file.writeBytes(seal(it.raw)) }
    } catch (e: GeneralSecurityException) {
        throw TransportError.Failed("The device key could not be read. Unlock the device and try again.")
    } catch (e: IOException) {
        throw TransportError.Failed("The device key could not be stored: ${e.message}")
    }

    private fun seal(raw: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
        return cipher.iv + cipher.doFinal(raw)
    }

    private fun open(sealed: ByteArray): ByteArray {
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.DECRYPT_MODE, wrappingKey(), GCMParameterSpec(TAG_BITS, sealed, 0, IV_BYTES))
        return cipher.doFinal(sealed, IV_BYTES, sealed.size - IV_BYTES)
    }

    private fun wrappingKey(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val spec = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUnlockedDeviceRequired(true)
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE).apply { init(spec) }.generateKey()
    }

    private companion object {
        const val KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "muxa-client-key"
        const val TRANSFORM = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
        const val TAG_BITS = 128
    }
}
