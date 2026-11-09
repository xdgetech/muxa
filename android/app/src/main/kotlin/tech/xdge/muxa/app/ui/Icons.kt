package tech.xdge.muxa.app.ui

import androidx.compose.material.icons.Icons as Material
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Article
import androidx.compose.material.icons.automirrored.filled.Assignment
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.AddBox
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Difference
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.FormatSize
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.LinkOff
import androidx.compose.material.icons.filled.LowPriority
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PanTool
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.QuestionAnswer
import androidx.compose.material.icons.filled.RestartAlt
import androidx.compose.material.icons.filled.Terminal
import androidx.compose.material.icons.filled.ViewList
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material.icons.filled.DisabledByDefault
import androidx.compose.material.icons.filled.Update
import androidx.compose.material.icons.filled.Newspaper
import androidx.compose.material.icons.filled.Lightbulb
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.LockOpen

/** One icon per concept, used everywhere that concept appears (as `Icon` on iOS). */
object Icons {
    val host = Material.Filled.Computer
    val session = Material.Filled.ViewList
    val repo = Material.Filled.Folder
    val document = Material.AutoMirrored.Filled.Article
    val image = Material.Filled.Image
    val chat = Material.AutoMirrored.Filled.Chat
    val agent = Material.Filled.Terminal
    val fit = Material.Filled.FormatSize
    val more = Material.Filled.MoreVert
    val history = Material.Filled.ArrowUpward
    val disconnect = Material.Filled.LinkOff
    val add = Material.Filled.Add
    val newTask = Material.Filled.AddBox
    val ask = Material.Filled.QuestionAnswer
    val closeTask = Material.Filled.DisabledByDefault
    val edit = Material.Filled.Edit
    val delete = Material.Filled.Delete
    val demo = Material.Filled.PlayCircle
    val leaveDemo = Material.AutoMirrored.Filled.Logout
    val warning = Material.Filled.Warning
    val waiting = Material.Filled.PanTool
    val done = Material.Filled.CheckCircle
    val send = Material.AutoMirrored.Filled.Send
    val back = Material.AutoMirrored.Filled.ArrowBack
    val close = Material.Filled.Close
    val list = Material.Filled.Menu
    val tasks = Material.AutoMirrored.Filled.Assignment
    val chevron = Material.AutoMirrored.Filled.KeyboardArrowRight
    val changes = Material.Filled.Difference
    val restartAgent = Material.Filled.RestartAlt
    val copy = Material.Filled.ContentCopy
    val newWindow = Material.AutoMirrored.Filled.OpenInNew
    val routine = Material.Filled.Update
    val briefing = Material.Filled.Newspaper
    val proposal = Material.Filled.Lightbulb
    /** Later: to the end of the queue, as Music's Play Last (ADR 0040). */
    val later = Material.Filled.LowPriority
    val check = Material.Filled.Check
    val run = Material.Filled.PlayArrow
    val settings = Material.Filled.Settings
    val moveUp = Material.Filled.ArrowUpward
    val moveDown = Material.Filled.ArrowDownward
    /** Protected: Delete Old keeps the item (ADR 0046). */
    val protect = Material.Filled.Lock
    val unprotect = Material.Filled.LockOpen
}
