package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.RowScope
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow

/** The top bar of the screens besides the main one: a title, Back when there is somewhere to go back to, actions. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppBar(
    title: String,
    onBack: (() -> Unit)? = null,
    backTag: String = "back",
    actions: @Composable RowScope.() -> Unit = {},
) {
    TopAppBar(
        title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        navigationIcon = {
            if (onBack != null) {
                IconButton(onClick = onBack, modifier = Modifier.testTag(backTag)) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                }
            }
        },
        actions = actions,
        colors =
            TopAppBarDefaults.topAppBarColors(
                containerColor = Palette.gray800,
                titleContentColor = Palette.gray200,
                navigationIconContentColor = Palette.gray300,
                actionIconContentColor = Palette.gray400,
            ),
    )
}

/** An entry of an [OverflowMenu]. */
class MenuItem(
    val label: String,
    val tag: String,
    val onClick: () -> Unit,
)

/** The ⋮ menu of a top bar. */
@Composable
fun OverflowMenu(vararg items: MenuItem) {
    Box {
        var open by remember { mutableStateOf(false) }
        IconButton(onClick = { open = true }, modifier = Modifier.testTag("menu")) {
            Icon(Icons.Default.MoreVert, contentDescription = "Menu")
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            for (item in items) {
                DropdownMenuItem(
                    text = { Text(item.label) },
                    onClick = {
                        open = false
                        item.onClick()
                    },
                    modifier = Modifier.testTag(item.tag),
                )
            }
        }
    }
}
