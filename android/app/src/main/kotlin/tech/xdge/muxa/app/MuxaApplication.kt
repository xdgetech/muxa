package tech.xdge.muxa.app

import android.app.Application
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.Job
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.bouncycastle.jce.provider.BouncyCastleProvider
import tech.xdge.muxa.app.next.NextModel
import tech.xdge.muxa.app.ui.ThemeStore
import tech.xdge.muxa.session.HostStore
import java.io.File
import java.security.Security
import kotlin.time.Duration.Companion.seconds

/**
 * Holds each window's [NextModel], so a connection outlives its Activity: a fold, a
 * resize or a recreation never drops it (ADR 0013, 0029).
 */
class MuxaApplication : Application() {
    lateinit var windows: Windows
        private set

    private val scope = MainScope()
    private var leaving: Job? = null

    override fun onCreate() {
        super.onCreate()
        // sshj signs Ed25519 with BouncyCastle. Android's own provider named "BC" is a
        // stripped copy without it, and sshj keeps whatever is registered under that name.
        Security.removeProvider(BouncyCastleProvider.PROVIDER_NAME)
        Security.insertProviderAt(BouncyCastleProvider(), 1)
        ThemeStore.start(this)
        // Every window shares the saved hosts and the device key, never a connection.
        val hosts = HostStore(File(filesDir, "hosts.json"))
        val keys = ClientKeyStore(this)
        val name = "muxa-android ${packageManager.getPackageInfo(packageName, 0).versionName}"
        windows = Windows { NextModel(MainScope(), hosts, keys, name) }
        ProcessLifecycleOwner.get().lifecycle.addObserver(Foreground())
    }

    /** A window's Activity is gone. One the user closed ends its connection now. */
    fun windowClosed(id: String, finished: Boolean) {
        val model = windows.closed(id, finished) ?: return
        scope.launch {
            model.suspend()
            model.scope.cancel()
        }
    }

    /**
     * The whole app, not one Activity: a recreation never stops it. Leaving the
     * foreground starts a grace period; after it every window's connection closes; the
     * server keeps each session for a resume, as when iOS suspends the app.
     */
    private inner class Foreground : DefaultLifecycleObserver {
        override fun onStart(owner: LifecycleOwner) {
            leaving?.cancel()
            leaving = null
            for (model in windows.all) scope.launch { model.resumeIfNeeded() }
        }

        override fun onStop(owner: LifecycleOwner) {
            leaving = scope.launch {
                delay(GRACE)
                for (model in windows.all) launch { model.suspend() }
            }
        }
    }

    private companion object {
        val GRACE = 30.seconds
    }
}
