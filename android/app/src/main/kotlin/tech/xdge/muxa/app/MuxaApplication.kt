package tech.xdge.muxa.app

import android.app.Application
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import kotlinx.coroutines.Job
import kotlinx.coroutines.MainScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.bouncycastle.jce.provider.BouncyCastleProvider
import java.security.Security
import kotlin.time.Duration.Companion.seconds

/**
 * Holds the app's one [AppModel], so the connection outlives any Activity: a fold, a
 * resize or a recreation never drops it (ADR 0013).
 */
class MuxaApplication : Application() {
    lateinit var model: AppModel
        private set

    private val scope = MainScope()
    private var leaving: Job? = null

    override fun onCreate() {
        super.onCreate()
        // sshj signs Ed25519 with BouncyCastle. Android's own provider named "BC" is a
        // stripped copy without it, and sshj keeps whatever is registered under that name.
        Security.removeProvider(BouncyCastleProvider.PROVIDER_NAME)
        Security.insertProviderAt(BouncyCastleProvider(), 1)
        model = AppModel(this, scope)
        ProcessLifecycleOwner.get().lifecycle.addObserver(Foreground())
    }

    /**
     * The whole app, not one Activity: a recreation never stops it. Leaving the
     * foreground starts a grace period; after it the bridge gets `bye`, as iOS
     * does when it suspends the app.
     */
    private inner class Foreground : DefaultLifecycleObserver {
        override fun onStart(owner: LifecycleOwner) {
            leaving?.cancel()
            leaving = null
            scope.launch { model.resume() }
        }

        override fun onStop(owner: LifecycleOwner) {
            leaving = scope.launch {
                delay(GRACE)
                model.suspend()
            }
        }
    }

    private companion object {
        val GRACE = 30.seconds
    }
}
