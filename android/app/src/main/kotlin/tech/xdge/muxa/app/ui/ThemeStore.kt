package tech.xdge.muxa.app.ui

import android.content.Context
import android.content.res.Configuration
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Log
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import tech.xdge.muxa.session.ResolvedTheme
import tech.xdge.muxa.session.TerminalImport
import tech.xdge.muxa.session.ThemeAppearance
import tech.xdge.muxa.session.ThemeFile
import tech.xdge.muxa.session.ThemePick
import tech.xdge.muxa.session.ThemeRegistry
import tech.xdge.muxa.session.ThemeToken
import java.io.ByteArrayOutputStream
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
 * The device's pick, its theme files and the theme they resolve to (ADR 0091 decision 7,
 * Amendment 1), without Android or Compose, so a JVM test can check them. User files
 * live in [folder]; [skipped] hears of each one that did not load.
 */
class ThemeChoice(
    private val bundled: List<ThemeFile>,
    private val folder: File,
    private val storage: PickStorage,
    system: ThemeAppearance,
    private val skipped: (ThemeRegistry.Failure) -> Unit = {},
) {
    var registry: ThemeRegistry = load()
        private set
    var system: ThemeAppearance = system
        private set
    var pick: ThemePick = storage.read()?.let(ThemePick::parse) ?: ThemePick.Fixed(ThemeRegistry.DEFAULT_ID)
        private set

    fun resolve(): ResolvedTheme = registry.resolve(pick, system)

    fun choose(id: String): ResolvedTheme = choose(ThemePick.Fixed(id))

    fun choose(pick: ThemePick): ResolvedTheme {
        this.pick = pick
        storage.write(pick.encoded())
        return resolve()
    }

    /** The system's light or dark mode, which an Automatic pick follows. */
    fun follow(system: ThemeAppearance): ResolvedTheme {
        this.system = system
        return resolve()
    }

    /**
     * Imports a muxa theme file (`.json`), an iTerm2 preset (`.itermcolors`) or a
     * Ghostty theme (anything else), as Apple does, and picks it. [fileName] gives
     * the kind and, without its extension, the name.
     */
    fun import(bytes: ByteArray, fileName: String): ResolvedTheme {
        val name = fileName.substringBeforeLast('.').ifEmpty { fileName }
        val file = when (fileName.substringAfterLast('.', "").lowercase()) {
            "json" -> ThemeFile.parse(bytes.decodeToString())
            "itermcolors" -> TerminalImport.iTerm(bytes, name)
            else -> TerminalImport.ghostty(bytes.decodeToString(), name)
        }
        if (file.id in registry.bundledIds) {
            throw TerminalImport.ImportError("The theme's id \"${file.id}\" is a built-in theme's. Change the id in the file.")
        }
        return save(file)
    }

    /** Saves a user theme, such as an edit, and picks it. */
    fun save(file: ThemeFile): ResolvedTheme {
        ThemeRegistry.save(file, folder)
        registry = load()
        return choose(file.id)
    }

    /** Removes a user theme's file. A pick that named it goes back to the default. */
    fun remove(id: String): ResolvedTheme {
        if (id in registry.bundledIds) return resolve()
        ThemeRegistry.remove(id, folder)
        registry = load()
        if (ThemeAppearance.entries.any { pick.id(it) == id }) return choose(ThemeRegistry.DEFAULT_ID)
        return resolve()
    }

    private fun load(): ThemeRegistry {
        val (user, failures) = ThemeRegistry.userFiles(folder)
        failures.forEach(skipped)
        return ThemeRegistry(bundled, user)
    }

    companion object {
        /** ADR 0091 Amendment 1: Automatic. */
        val AUTOMATIC = ThemePick.System(light = "muxa-light", dark = ThemeRegistry.DEFAULT_ID)
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

    /** A theme file is a few KiB; a bigger one is not a theme. */
    private const val MAX_IMPORT_BYTES = 1 shl 20

    private val bundled: List<ThemeFile> by lazy(::loadBundled)
    private var choice: ThemeChoice? = null
    private var state by mutableStateOf(AppTheme(ThemeRegistry(bundled).resolve(ThemeRegistry.DEFAULT_ID)))
    private var pickState by mutableStateOf<ThemePick>(ThemePick.Fixed(ThemeRegistry.DEFAULT_ID))
    private var registryState by mutableStateOf(ThemeRegistry(bundled))

    val current: AppTheme get() = state

    /** What Settings marks: a theme, or Automatic. */
    val pick: ThemePick get() = pickState

    /** The themes Settings lists, the built-ins first. */
    val registry: ThemeRegistry get() = registryState

    /** Reads the user's theme files and the saved pick. Call once, before any UI. */
    fun start(context: Context) {
        if (choice != null) return
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val storage = object : PickStorage {
            override fun read(): String? = prefs.getString(KEY, null)
            override fun write(value: String) {
                prefs.edit().putString(KEY, value).apply()
            }
        }
        val night = context.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK
        val system = if (night == Configuration.UI_MODE_NIGHT_NO) ThemeAppearance.LIGHT else ThemeAppearance.DARK
        val started = ThemeChoice(bundled, File(context.filesDir, "themes"), storage, system) {
            Log.w(TAG, "Theme file ${it.file.name} was skipped: ${it.error.message}")
        }
        choice = started
        publish(started, started.resolve())
    }

    fun choose(id: String) = choose(ThemePick.Fixed(id))

    fun choose(pick: ThemePick) {
        val started = choice ?: return
        publish(started, started.choose(pick))
    }

    /** The system's dark mode, read by the root composable; only Automatic changes with it. */
    fun follow(dark: Boolean) {
        val started = choice ?: return
        val system = if (dark) ThemeAppearance.DARK else ThemeAppearance.LIGHT
        if (started.system != system) publish(started, started.follow(system))
    }

    /** Imports the document at [uri] and picks it; the error to show when it cannot. */
    suspend fun import(context: Context, uri: Uri): String? {
        val started = choice ?: return "Themes are not loaded yet."
        return try {
            val (name, bytes) = withContext(Dispatchers.IO) { read(context, uri) }
            publish(started, started.import(bytes, name))
            null
        } catch (e: TerminalImport.ImportError) {
            e.message
        } catch (e: ThemeFile.LoadError) {
            e.message
        } catch (e: IOException) {
            Log.w(TAG, "A theme import failed: ${e.message}")
            "The file could not be read. Pick it again, or copy it to this device first."
        } catch (e: SecurityException) {
            Log.w(TAG, "A theme import was not allowed: ${e.message}")
            "muxa may not read that file. Pick it again from Files."
        }
    }

    /** Saves an edit of the picked theme and picks it; the error to show when it cannot. */
    fun save(file: ThemeFile): String? {
        val started = choice ?: return "Themes are not loaded yet."
        return try {
            publish(started, started.save(file))
            null
        } catch (e: ThemeFile.LoadError) {
            e.message
        } catch (e: IOException) {
            Log.e(TAG, "Theme ${file.id} was not saved: ${e.message}")
            "The colors could not be saved. Check the device has free space, then save again."
        }
    }

    fun remove(id: String) {
        val started = choice ?: return
        try {
            publish(started, started.remove(id))
        } catch (e: IOException) {
            Log.e(TAG, "Theme $id was not removed: ${e.message}")
        }
    }

    private fun publish(started: ThemeChoice, theme: ResolvedTheme) {
        registryState = started.registry
        pickState = started.pick
        state = AppTheme(theme)
    }

    /** The document's file name and its bytes, at most [MAX_IMPORT_BYTES]. */
    private fun read(context: Context, uri: Uri): Pair<String, ByteArray> {
        val resolver = context.contentResolver
        val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getString(0) else null
        } ?: uri.lastPathSegment ?: "Imported"
        val bytes = resolver.openInputStream(uri)?.use { input ->
            val out = ByteArrayOutputStream()
            val buffer = ByteArray(8192)
            while (true) {
                val n = input.read(buffer)
                if (n < 0) break
                out.write(buffer, 0, n)
                if (out.size() > MAX_IMPORT_BYTES) throw TerminalImport.ImportError("The file is too big to be a theme.")
            }
            out.toByteArray()
        } ?: throw IOException("The document has no content")
        return name to bytes
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
