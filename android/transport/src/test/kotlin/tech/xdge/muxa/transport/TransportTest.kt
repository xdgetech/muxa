package tech.xdge.muxa.transport

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.nio.file.Files

class TransportTest {
    @get:Rule
    val dir = TemporaryFolder()

    @Test
    fun clientKeyRoundTripsAndFormatsForAuthorizedKeys() {
        val key = ClientKey.generate()
        val again = ClientKey.fromRaw(key.raw)
        assertEquals(key.authorizedKey("muxa"), again.authorizedKey("muxa"))
        assertTrue(key.authorizedKey("muxa").startsWith("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5"))
        assertTrue(key.authorizedKey("muxa").endsWith(" muxa"))
        // The same key as java.security objects, as sshj signs with it.
        assertEquals("Ed25519", key.keyPair().private.algorithm)
    }

    @Test
    fun hostKeyFingerprintMatchesSshKeygen() {
        // A throwaway key; the expected value is what `ssh-keygen -lf` printed for it.
        val key = HostKey.parse("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICmVLvUbABi5WZoMHwkY/jy0DxNDGNMw2TRDgtxMPf9x test")
        assertEquals("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICmVLvUbABi5WZoMHwkY/jy0DxNDGNMw2TRDgtxMPf9x", key.openSSH)
        assertEquals("SHA256:r6zfLbrOjsc3OA59oKVabd7VJ5ZMt25hyJkS3EJEuGo", key.fingerprint)
    }

    @Test
    fun processBytesCarriesRawBytesBothWays() = runBlocking {
        val channel = ProcessBytes("cat")
        // Not UTF-8: bytes must pass through unchanged.
        val sent = byteArrayOf(0, 0xff.toByte(), 0x80.toByte(), '\n'.code.toByte())
        channel.send(sent)
        val received = java.io.ByteArrayOutputStream()
        while (received.size() < sent.size) received.write(channel.bytes.receive())
        assertTrue(sent.contentEquals(received.toByteArray()))
        channel.close()
    }

    @Test
    fun theLineIsRestrictedToTheBridge() {
        val line = KeySetup.authorizedLine(ClientKey.generate(), "~/.local/bin/muxa-bridge", "muxa-android")
        assertTrue(
            line.startsWith(
                "command=\"~/.local/bin/muxa-bridge\",no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding ssh-ed25519 ",
            ),
        )
        assertTrue(line.endsWith(" muxa-android"))
        try {
            KeySetup.authorizedLine(ClientKey.generate(), "x\" ,permitopen=\"*", "c")
            throw AssertionError("a quote must be refused")
        } catch (_: TransportError.Failed) {
        }
    }

    /** Runs the setup script the way sshd would: through the user's shell, in a fresh HOME. */
    @Test
    fun theScriptAddsOnceAndKeepsExistingLines() {
        for (shell in listOf("/bin/sh", "/bin/zsh", "/bin/bash").filter { File(it).exists() }) {
            val home = dir.newFolder()
            val line = "command=\"it's ~/x\",no-pty ssh-ed25519 AAAA muxa-android"
            val script = KeySetup.script(line)
            fun run(): KeySetup.Outcome {
                val p = ProcessBuilder(shell, "-c", script).apply {
                    environment().clear()
                    environment()["HOME"] = home.path
                    environment()["PATH"] = "/usr/bin:/bin"
                }.start()
                val out = p.inputStream.readBytes().toString(Charsets.UTF_8)
                assertEquals(shell, 0, p.waitFor())
                return KeySetup.outcome(out)
            }

            assertEquals(shell, KeySetup.Outcome.ADDED, run())
            val ssh = home.resolve(".ssh")
            val file = ssh.resolve("authorized_keys")
            assumeTrue(Files.getFileStore(file.toPath()).supportsFileAttributeView("posix"))
            assertEquals("rwx------", java.nio.file.attribute.PosixFilePermissions.toString(Files.getPosixFilePermissions(ssh.toPath())))
            assertEquals("rw-------", java.nio.file.attribute.PosixFilePermissions.toString(Files.getPosixFilePermissions(file.toPath())))
            assertEquals(line + "\n", file.readText())

            // An existing file without a final newline keeps its last line intact.
            file.writeText("ssh-ed25519 BBBB other")
            assertEquals(shell, KeySetup.Outcome.ADDED, run())
            assertEquals(shell, KeySetup.Outcome.ALREADY_PRESENT, run())
            assertEquals("ssh-ed25519 BBBB other\n$line\n", file.readText())
        }
    }
}
