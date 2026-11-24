package tech.xdge.muxa.app.ui

import android.content.Context
import android.content.res.Configuration
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import tech.xdge.muxa.session.ResolvedTheme
import tech.xdge.muxa.session.ThemeAppearance
import tech.xdge.muxa.session.ThemeFile
import tech.xdge.muxa.session.ThemePick
import tech.xdge.muxa.session.ThemeRegistry
import tech.xdge.muxa.session.ThemeToken
import java.io.File
import java.io.IOException

/** The resolved theme as Compose colors, one per token. */
class AppTheme(val resolved: ResolvedTheme) {
    private val colors = ThemeToken.entries.map { resolved[it].let { c -> Color(c.r, c.g, c.b) } }

    operator fun get(token: ThemeToken): Color = colors[token.ordinal]
    val isDark: Boolean get() = resolved.appearance == ThemeAppearance.DARK
}

/** Where the device keeps its pick: SharedPreferences on a device, a map in tests. */
interface PickStorage {
    fun read(): String?
    fun write(value: String)
}

/**
 * The device's pick and the theme it resolves to (ADR 0091 decision 7), without
 * Android or Compose, so a JVM test can check that a pick is kept.
 */
class ThemeChoice(val registry: ThemeRegistry, private val storage: PickStorage, private val system: ThemeAppearance) {
    var pick: ThemePick = storage.read()?.let(ThemePick::parse) ?: ThemePick.Fixed(ThemeRegistry.DEFAULT_ID)
        private set

    fun resolve(): ResolvedTheme = registry.resolve(pick, system)

    fun choose(id: String): ResolvedTheme {
        pick = ThemePick.Fixed(id)
        storage.write(pick.encoded())
        return resolve()
    }
}

/**
 * The app's one theme. Composition reads [current] as state, so a new pick
 * recomposes every screen; Views read it too, and are told by their composable.
 */
object ThemeStore {
    private const val TAG = "muxa.theme"
    private const val PREFS = "theme"
    private const val KEY = "theme.pick"

    private val bundled: List<ThemeFile> by lazy(::loadBundled)
    private var choice: ThemeChoice? = null
    private var state by mutableStateOf(AppTheme(ThemeRegistry(bundled).resolve(ThemeRegistry.DEFAULT_ID)))

    val current: AppTheme get() = state

    /** The themes Settings lists, the built-ins first. */
    val registry: ThemeRegistry get() = choice?.registry ?: ThemeRegistry(bundled)

    /** Reads the user's theme files and the saved pick. Call once, before any UI. */
    fun start(context: Context) {
        if (choice != null) return
        val (user, failures) = ThemeRegistry.userFiles(File(context.filesDir, "themes"))
        failures.forEach { Log.w(TAG, "Theme file ${it.file.name} was skipped: ${it.error.message}") }
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val storage = object : PickStorage {
            override fun read(): String? = prefs.getString(KEY, null)
            override fun write(value: String) {
                prefs.edit().putString(KEY, value).apply()
            }
        }
        val night = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
        val system = if (night == Configuration.UI_MODE_NIGHT_NO) ThemeAppearance.LIGHT else ThemeAppearance.DARK
        val started = ThemeChoice(ThemeRegistry(bundled, user), storage, system)
        choice = started
        state = AppTheme(started.resolve())
    }

    fun choose(id: String) {
        val started = choice ?: return
        state = AppTheme(started.choose(id))
    }

    // Without them the resolver still gives muxa Dark from its own constants.
    private fun loadBundled(): List<ThemeFile> = try {
        ThemeRegistry.bundledFiles()
    } catch (e: IOException) {
        Log.e(TAG, "The bundled themes did not load: ${e.message}")
        emptyList()
    } catch (e: ThemeFile.LoadError) {
        Log.e(TAG, "A bundled theme did not load: ${e.message}")
        emptyList()
    }
}
