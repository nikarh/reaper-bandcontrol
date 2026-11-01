package dev.nikarh.bandcontrol.app.ui

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Undo
import androidx.compose.material.icons.filled.BookmarkAdd
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.nikarh.bandcontrol.core.state.MixLevels
import dev.nikarh.bandcontrol.core.state.MixPresets
import dev.nikarh.bandcontrol.core.state.MixPresets.Companion.UNSAVED
import kotlinx.coroutines.delay

/** After loading, the levels take a moment to come back from REAPER: not "modified" meanwhile. */
private const val SETTLE_MS = 1_500L

/** A dialog asking for a preset name: to save the mix, or to rename [renaming]. */
private data class NameDialog(
    val renaming: String? = null,
)

/**
 * The presets of one mix: tap one to load it (the unsaved changes stay one tap away, so presets can be compared),
 * Revert to undo the changes to the loaded preset, Save to keep the mix. Long-press a preset to update, rename or
 * delete it.
 */
@Composable
fun PresetBar(
    presets: MixPresets,
    current: MixLevels,
    enabled: Boolean,
    onLoad: (String) -> Boolean,
    onSave: (String) -> Unit,
    onRename: (String, String) -> Unit,
    onDelete: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    var settling by remember { mutableStateOf(false) }
    LaunchedEffect(settling) {
        if (settling) {
            delay(SETTLE_MS)
            settling = false
        }
    }
    val load = { name: String -> if (onLoad(name)) settling = true }
    val modified = !settling && presets.modified(current)
    var dialog by remember { mutableStateOf<NameDialog?>(null) }
    var deleting by remember { mutableStateOf<String?>(null) }

    Row(
        modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 8.dp)
            .testTag("presets"),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val active = presets.active
        if (modified && active != null && active != UNSAVED) {
            PresetChip("Revert", enabled, icon = Icons.AutoMirrored.Filled.Undo, modifier = Modifier.testTag("preset-revert")) {
                load(active)
            }
        }
        for (preset in presets.presets) {
            val selected = preset.name == active
            Box {
                var menu by remember { mutableStateOf(false) }
                PresetChip(
                    if (selected && modified) "${preset.name} •" else preset.name,
                    enabled,
                    selected = selected,
                    onLongClick = { menu = true },
                    modifier =
                        Modifier
                            .semantics {
                                customActions =
                                    listOf(
                                        CustomAccessibilityAction("Update with this mix") {
                                            onSave(preset.name)
                                            true
                                        },
                                        CustomAccessibilityAction("Rename") {
                                            dialog = NameDialog(preset.name)
                                            true
                                        },
                                        CustomAccessibilityAction("Delete") {
                                            deleting = preset.name
                                            true
                                        },
                                    )
                            }.testTag("preset-${preset.name}"),
                ) { load(preset.name) }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(
                        text = { Text("Update with this mix") },
                        onClick = {
                            menu = false
                            onSave(preset.name)
                        },
                        modifier = Modifier.testTag("preset-update"),
                    )
                    DropdownMenuItem(text = { Text("Rename…") }, onClick = {
                        menu = false
                        dialog = NameDialog(preset.name)
                    }, modifier = Modifier.testTag("preset-rename"))
                    DropdownMenuItem(
                        text = { Text("Delete", color = Color(0xFFF87171)) },
                        onClick = {
                            menu = false
                            deleting = preset.name
                        },
                        modifier = Modifier.testTag("preset-delete"),
                    )
                }
            }
        }
        if (presets.unsaved != null) {
            PresetChip(
                "Unsaved",
                enabled,
                selected = active == UNSAVED,
                italic = true,
                modifier = Modifier.testTag("preset-unsaved"),
            ) { load(UNSAVED) }
        }
        PresetChip(
            if (presets.presets.isEmpty()) "Save as preset" else "Save",
            // Saving needs no connection: it stores what the phone shows.
            enabled = true,
            icon = Icons.Default.BookmarkAdd,
            modifier = Modifier.testTag("preset-save"),
        ) { dialog = NameDialog() }
    }

    dialog?.let { d ->
        val initial =
            d.renaming ?: presets.active?.takeIf { it != UNSAVED && modified } ?: presets.newName()
        PresetNameDialog(
            title = if (d.renaming != null) "Rename preset" else "Save mix as preset",
            initial = initial,
            confirm = if (d.renaming != null) "Rename" else "Save",
            replaces = { name -> name != d.renaming && presets.preset(name) != null },
            onConfirm = { name ->
                if (d.renaming != null) onRename(d.renaming, name) else onSave(name)
                dialog = null
            },
            onDismiss = { dialog = null },
        )
    }
    deleting?.let { name ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            containerColor = Palette.gray800,
            title = { Text("Delete “$name”?", color = Palette.gray200) },
            text = { Text("The mix stays as it is; only the saved preset goes.", color = Palette.gray400) },
            confirmButton = {
                TextButton(onClick = {
                    onDelete(name)
                    deleting = null
                }, modifier = Modifier.testTag("preset-delete-confirm")) { Text("Delete", color = Color(0xFFF87171)) }
            },
            dismissButton = { TextButton(onClick = { deleting = null }) { Text("Cancel", color = Palette.blue500) } },
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PresetChip(
    label: String,
    enabled: Boolean,
    modifier: Modifier = Modifier,
    selected: Boolean = false,
    italic: Boolean = false,
    icon: ImageVector? = null,
    onLongClick: (() -> Unit)? = null,
    onClick: () -> Unit,
) {
    val haptics = LocalHapticFeedback.current
    Row(
        modifier
            .height(32.dp)
            .clip(RoundedCornerShape(16.dp))
            .background(if (selected) Palette.blue500.copy(alpha = 0.25f) else Palette.gray700)
            // Managing presets works without a connection, only loading one needs it.
            .combinedClickable(
                enabled = enabled || onLongClick != null,
                onClick = { if (enabled) onClick() },
                onLongClick =
                    onLongClick?.let {
                        {
                            haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                            it()
                        }
                    },
            ).semantics {
                role = Role.Button
                this.selected = selected
            }.padding(horizontal = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val color =
            when {
                !enabled -> Palette.gray500
                selected -> Color.White
                else -> Palette.gray200
            }
        if (icon != null) {
            Icon(icon, contentDescription = null, tint = if (enabled) Palette.blue500 else Palette.gray500, modifier = Modifier.size(16.dp))
            Spacer(Modifier.width(6.dp))
        }
        Text(
            label,
            color = color,
            fontSize = 13.sp,
            maxLines = 1,
            fontWeight = if (selected) FontWeight.SemiBold else FontWeight.Normal,
            fontStyle = if (italic) FontStyle.Italic else FontStyle.Normal,
        )
    }
}

@Composable
private fun PresetNameDialog(
    title: String,
    initial: String,
    confirm: String,
    replaces: (String) -> Boolean,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    // Selected, so typing replaces the suggestion.
    var value by remember { mutableStateOf(TextFieldValue(initial, TextRange(0, initial.length))) }
    val name = value.text.trim()
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    val done = { if (name.isNotEmpty()) onConfirm(name) }
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = Palette.gray800,
        title = { Text(title, color = Palette.gray200) },
        text = {
            Column {
                OutlinedTextField(
                    value = value,
                    onValueChange = { value = it },
                    label = { Text("Name") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { done() }),
                    modifier = Modifier.fillMaxWidth().focusRequester(focus).testTag("preset-name"),
                )
                if (replaces(name)) {
                    Spacer(Modifier.height(6.dp))
                    Text("Replaces the saved “$name”", color = Palette.yellow600, fontSize = 13.sp)
                }
            }
        },
        confirmButton = {
            TextButton(onClick = done, enabled = name.isNotEmpty(), modifier = Modifier.testTag("preset-confirm")) {
                Text(confirm, color = if (name.isNotEmpty()) Palette.blue500 else Palette.gray500)
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel", color = Palette.blue500) } },
    )
}
