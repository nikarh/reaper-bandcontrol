package dev.nikarh.bandcontrol.app.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

/** Tailwind palette used by the original web UI. */
object Palette {
    val gray200 = Color(0xFFE5E7EB)
    val gray300 = Color(0xFFD1D5DB)
    val gray400 = Color(0xFF9CA3AF)
    val gray500 = Color(0xFF6B7280)
    val gray600 = Color(0xFF4B5563)
    val gray700 = Color(0xFF374151)
    val gray800 = Color(0xFF1F2937)
    val gray900 = Color(0xFF111827)
    val red500 = Color(0xFFEF4444)
    val red600 = Color(0xFFDC2626)
    val blue500 = Color(0xFF3B82F6)
    val emerald300 = Color(0xFF6EE7B7)
    val emerald400 = Color(0xFF34D399)
    val yellow600 = Color(0xFFCA8A04)
    val slate200 = Color(0xFFE2E8F0)
}

/** `0xRRGGBB` from the protocol. */
fun rgb(color: Int?): Color? = color?.let { Color(0xFF000000 or (it.toLong() and 0xFFFFFF)) }

/** Black or white, whichever reads better on [background] (same rule as the web UI). */
fun textOn(background: Color?): Color {
    if (background == null) return Color.White
    val r = background.red * 255
    val g = background.green * 255
    val b = background.blue * 255
    return if (r * r + g * g + b * b > 255f * 255f * 3f / 2f) Color.Black else Color.White
}

@Composable
fun BandControlTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme =
            darkColorScheme(
                primary = Palette.blue500,
                background = Palette.gray900,
                surface = Palette.gray800,
                surfaceVariant = Palette.gray700,
                onBackground = Palette.gray200,
                onSurface = Palette.gray200,
                onSurfaceVariant = Palette.gray400,
                error = Palette.red500,
            ),
        content = content,
    )
}
