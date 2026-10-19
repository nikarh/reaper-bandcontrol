package dev.nikarh.bandcontrol.core

import org.junit.Assert.assertEquals
import org.junit.Test

class ComputersTest {
    private val saved = listOf("rehearsal", "stage", "home")

    @Test
    fun oneSavedComputerIsUsedRightAway() {
        assertEquals(ComputerChoice.Connect("stage"), chooseComputer(listOf("stage"), emptySet(), null, settled = false))
    }

    @Test
    fun theOnlyOneInRangeConnects() {
        assertEquals(ComputerChoice.Connect("stage"), chooseComputer(saved, setOf("stage"), null, settled = true))
        // Before the first scan settled another one may still show up.
        assertEquals(ComputerChoice.Wait, chooseComputer(saved, setOf("stage"), null, settled = false))
        assertEquals(ComputerChoice.Connect("stage"), chooseComputer(listOf("stage", "home"), setOf("stage", "home"), "stage", false))
    }

    @Test
    fun severalInRangeAsk() {
        assertEquals(ComputerChoice.Wait, chooseComputer(saved, setOf("stage", "home"), null, settled = false))
        assertEquals(ComputerChoice.Ask(listOf("stage", "home")), chooseComputer(saved, setOf("home", "stage"), null, settled = true))
        assertEquals(ComputerChoice.Wait, chooseComputer(saved, emptySet(), null, settled = true))
        assertEquals(ComputerChoice.Ask(saved), chooseComputer(saved, saved.toSet(), null, settled = false))
        // Unpaired computers don't count.
        assertEquals(ComputerChoice.Connect("home"), chooseComputer(saved, setOf("home", "other"), null, settled = true))
    }

    @Test
    fun aChoiceSticks() {
        assertEquals(ComputerChoice.Connect("home"), chooseComputer(saved, setOf("stage"), "home", settled = true))
        // A forgotten computer is no choice.
        assertEquals(ComputerChoice.Connect("stage"), chooseComputer(saved, setOf("stage"), "gone", settled = true))
    }

    @Test
    fun noticesForNewComputers() {
        assertEquals(setOf("stage"), newlyNearby(setOf("home"), setOf("home", "stage"), inUse = "home"))
        assertEquals(emptySet<String>(), newlyNearby(emptySet(), setOf("home"), inUse = "home"))
        assertEquals(emptySet<String>(), newlyNearby(setOf("stage"), setOf("stage"), inUse = "home"))
    }
}
