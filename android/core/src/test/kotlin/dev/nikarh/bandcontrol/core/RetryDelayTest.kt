package dev.nikarh.bandcontrol.core

import org.junit.Assert.assertEquals
import org.junit.Test

class RetryDelayTest {
    private fun delay(
        failures: Int,
        sessionMs: Long? = null,
        rejected: Boolean = false,
    ) = retryDelay(failures, rejected, sessionMs, retryDelayMs = 1_000, maxRetryDelayMs = 5_000)

    @Test
    fun backsOffUpToTheMaximum() {
        assertEquals(listOf(1_000L, 2_000L, 4_000L, 5_000L, 5_000L), (0..4).map { delay(it) })
        assertEquals(5_000L, delay(100))
    }

    @Test
    fun aLostWorkingSessionRetriesAtOnce() {
        assertEquals(0L, delay(0, sessionMs = 60_000))
        assertEquals(0L, delay(0, sessionMs = STABLE_SESSION_MS))
    }

    @Test
    fun aSessionThatDroppedRightAwayBacksOff() {
        // E.g. a computer that accepts and drops the phone at once: no busy loop.
        assertEquals(1_000L, delay(0, sessionMs = 200))
    }

    @Test
    fun aRejectedPairingWaitsTheLongest() {
        assertEquals(5_000L, delay(0, sessionMs = 60_000, rejected = true))
    }
}
