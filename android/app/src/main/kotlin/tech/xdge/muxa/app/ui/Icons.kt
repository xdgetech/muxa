package tech.xdge.muxa.app.ui

import androidx.compose.material.icons.Icons as Material
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Article
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.automirrored.filled.Logout
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.AddBox
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.FormatSize
import androidx.compose.material.icons.filled.Image
import androidx.compose.material.icons.filled.LinkOff
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.PanTool
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.Terminal
import androidx.compose.material.icons.filled.ViewList
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material.icons.filled.DisabledByDefault

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
    val disconnect = Material.Filled.LinkOff
    val add = Material.Filled.Add
    val newTask = Material.Filled.AddBox
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
}
