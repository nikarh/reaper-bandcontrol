package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.core.state.MixPresets.Companion.UNSAVED
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PresetsTest {
    private fun levels(click: Float) = MixLevels(1f, listOf(SourceLevel("Click", click, false), SourceLevel("Bass", 1f, false)))

    private val quiet = levels(0.25f)
    private val loud = levels(1f)
    private val tweaked = levels(0.6f)

    @Test
    fun savingMakesAPresetActive() {
        val p = MixPresets().save("Rehearsal", quiet)
        assertEquals(listOf(MixPreset("Rehearsal", quiet)), p.presets)
        assertEquals("Rehearsal", p.active)
        assertFalse(p.modified(quiet))
        assertTrue(p.modified(tweaked))
        // Saving under the same name replaces it.
        assertEquals(listOf(MixPreset("Rehearsal", tweaked)), p.save("Rehearsal", tweaked).presets)
    }

    @Test
    fun revertingLoadsTheActivePresetAgain() {
        val p = MixPresets().save("Rehearsal", quiet)
        val (after, apply) = p.load("Rehearsal", tweaked)!!
        assertEquals(quiet, apply)
        // The tweaks were made on top of the preset that is reverted to: kept as unsaved, they can come back.
        assertEquals(tweaked, after.unsaved)
    }

    @Test
    fun loadingAnotherPresetKeepsUnsavedChanges() {
        val p = MixPresets().save("Quiet", quiet).save("Loud", loud)
        val (compared, apply) = p.load("Quiet", tweaked)!!
        assertEquals(quiet, apply)
        assertEquals(tweaked, compared.unsaved)
        assertEquals("Quiet", compared.active)
        // Back and forth between presets doesn't overwrite the kept changes: the mix matches a preset each time.
        val (again, _) = compared.load("Loud", quiet)!!
        assertEquals(tweaked, again.unsaved)
        val (back, restored) = again.load(UNSAVED, loud)!!
        assertEquals(tweaked, restored)
        assertEquals(UNSAVED, back.active)
        assertFalse(back.modified(tweaked))
    }

    @Test
    fun aMixMatchingAPresetNeedsNoKeeping() {
        val p = MixPresets().save("Quiet", quiet).save("Loud", loud)
        assertNull(p.load("Loud", quiet)!!.first.unsaved)
        assertNull(p.load("Missing", quiet))
    }

    @Test
    fun savingTheUnsavedChangesDropsTheCopy() {
        val p = MixPresets().save("Quiet", quiet).load("Quiet", tweaked)!!.first
        assertEquals(tweaked, p.unsaved)
        assertNull(p.save("Tweaked", tweaked).unsaved)
    }

    @Test
    fun renameAndDelete() {
        val p = MixPresets().save("A", quiet).save("B", loud)
        val renamed = p.rename("B", "C")
        assertEquals(listOf("A", "C"), renamed.presets.map { it.name })
        assertEquals("C", renamed.active)
        // Renaming onto another name replaces that preset.
        assertEquals(listOf(MixPreset("A", loud)), p.rename("B", "A").presets)
        val deleted = renamed.delete("C")
        assertEquals(listOf("A"), deleted.presets.map { it.name })
        assertNull(deleted.active)
        assertEquals("Preset 2", deleted.newName())
        assertEquals("Preset 3", MixPresets().save("Preset 2", quiet).save("x", quiet).newName())
    }

    @Test
    fun storedAsJson() {
        val presets =
            Presets()
                .with("Drums", MixPresets().save("Quiet", quiet).load("Quiet", tweaked)!!.first)
                .with("Bass", MixPresets().save("Loud", loud))
                .copy(mine = "Bass", listening = Listening(mine = "Bass", to = "Drums", before = MixLevels(null, quiet.sources)))
        assertEquals(presets, Presets.fromJson(presets.toJson()))
        assertEquals(Presets(), Presets.fromJson(null))
        assertEquals(Presets(), Presets.fromJson("not json"))
        assertEquals(Presets(), Presets.fromJson("""{"mixes": {"Drums": {"presets": [{"name": 1}]}}}"""))
    }
}
