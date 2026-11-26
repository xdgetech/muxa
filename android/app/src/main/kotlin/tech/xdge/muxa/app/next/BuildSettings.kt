package tech.xdge.muxa.app.next

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import tech.xdge.muxa.app.ui.Icons
import tech.xdge.muxa.v2.Patch
import tech.xdge.muxa.v2.SettingsBuild
import tech.xdge.muxa.v2.SettingsOp

/** Settings' row that opens the build disk (ADR 0102). Hidden when the host is older and has none. */
@Composable
fun BuildRow(editor: SettingsEditor, open: () -> Unit) {
    val build = editor.settings?.build ?: return
    SettingsHeader("Build disk")
    SettingsCard {
        NavRow("Build disk", build.dir ?: "Off", "settings.build", open)
    }
}

/** Settings › Build disk: the host's `[build]` folder and how long an ended Task's build output stays. */
@Composable
fun BuildSettings(editor: SettingsEditor, build: SettingsBuild, back: () -> Unit) {
    val save = rememberInlineSave(editor)
    val on = !editor.readOnly
    Column(Modifier.testTag("settings.build.page")) {
        Row(Modifier.fillMaxWidth().padding(start = 4.dp, end = 14.dp, top = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            IconButton(back, Modifier.testTag("build.back")) { Icon(Icons.back, "Settings") }
            Text("Build disk", fontSize = 20.sp, fontWeight = FontWeight.SemiBold)
        }
        SettingsCard {
            Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                SavedField(build.dir.orEmpty(), "Folder", "settings.build.dir", on, "Off") {
                    save.run(listOf(SettingsOp.BuildUpdate(dir = patchOf(it))))
                }
                SavedField(build.dropAfter.orEmpty(), "Drop build output after", "settings.build.drop", on, DEFAULT_DROP_AFTER) {
                    save.run(listOf(SettingsOp.BuildUpdate(dropAfter = patchOf(it))))
                }
            }
        }
        BuildStatus(build)
        SettingsNote(
            "Each Task worktree's build output (target/ and the Workspace's build paths) goes to its own folder here. " +
                "After an ended Task waits this long, its build output is removed; its work stays.",
        )
        save.Error()
    }
}

@Composable
private fun BuildStatus(build: SettingsBuild) {
    if (build.dir == null) return
    if (build.dirFound) {
        SettingsNote("The host found this folder.", Modifier.testTag("settings.build.found"))
    } else {
        SettingsWarning(
            "The host cannot find this folder. Is the disk mounted? Tasks build in their worktree until it is.",
            "settings.build.missing",
        )
    }
}

/** An empty field clears the setting. */
private fun patchOf(text: String): Patch<String> = text.trim().ifEmpty { null }?.let { Patch.Set(it) } ?: Patch.Clear

/** Why a build path cannot be added: the host takes only relative paths of plain names. */
fun buildPathProblem(path: String): String? {
    val parts = path.split('/')
    val plain = !path.startsWith("/") && parts.none { it.isEmpty() || it == "." || it == ".." || it == "~" }
    return if (plain) null else "Write a path inside the worktree, such as target or web/.next."
}

private const val DEFAULT_DROP_AFTER = "3d"
