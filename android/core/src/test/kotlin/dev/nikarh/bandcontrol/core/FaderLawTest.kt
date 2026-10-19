package dev.nikarh.bandcontrol.core

import org.junit.Assert.assertEquals
import org.junit.Test

class FaderLawTest {
    @Test
    fun positions() {
        assertEquals(0f, FaderLaw.gainToPosition(0f))
        assertEquals(0f, FaderLaw.dbToPosition(-90f))
        assertEquals(0.25f, FaderLaw.dbToPosition(-20f), 1e-6f)
        assertEquals(0.71875f, FaderLaw.unityPosition, 1e-6f)
        assertEquals(1f, FaderLaw.gainToPosition(FaderLaw.maxGain), 1e-6f)
        assertEquals(1f, FaderLaw.gainToPosition(10f))
        assertEquals(3.981f, FaderLaw.maxGain, 1e-3f)
    }

    @Test
    fun roundTrips() {
        for (db in listOf(-59f, -40f, -20f, -6f, 0f, 6.02f, 12f)) {
            assertEquals(db, FaderLaw.positionToDb(FaderLaw.dbToPosition(db)), 1e-3f)
        }
        // +6 dB inputs keep their gain instead of being clamped to 0 dB.
        assertEquals(1.995f, FaderLaw.positionToGain(FaderLaw.gainToPosition(1.995f)), 1e-3f)
        assertEquals(0f, FaderLaw.positionToGain(0f))
    }

    @Test
    fun formatting() {
        assertEquals("-inf", FaderLaw.format(FaderLaw.gainToDb(0f)))
        assertEquals("0.0", FaderLaw.format(-0.01f))
        assertEquals("+6.0", FaderLaw.format(6.02f))
        assertEquals("-12.0", FaderLaw.format(FaderLaw.gainToDb(0.25f)))
    }
}
