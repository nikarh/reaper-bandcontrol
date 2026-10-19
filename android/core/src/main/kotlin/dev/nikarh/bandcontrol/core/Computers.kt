package dev.nikarh.bandcontrol.core

/** What to do about the paired computers in range. Computers are identified by their pairing code. */
sealed interface ComputerChoice {
    /** Nothing to decide yet: none in range, or still looking. */
    data object Wait : ComputerChoice

    data class Connect(
        val id: String,
    ) : ComputerChoice

    /** Several are in range and none was chosen: the user picks one. */
    data class Ask(
        val ids: List<String>,
    ) : ComputerChoice
}

/**
 * Which computer to use. A computer chosen in this session ([selected]) sticks; otherwise a single paired computer is
 * used right away, exactly one in range connects automatically, and several in range are offered to the user once
 * [settled] (the first scan had time to find them all).
 */
fun chooseComputer(
    saved: List<String>,
    nearby: Set<String>,
    selected: String?,
    settled: Boolean,
): ComputerChoice {
    if (selected != null && selected in saved) return ComputerChoice.Connect(selected)
    if (saved.size == 1) return ComputerChoice.Connect(saved.single())
    val inRange = saved.filter { it in nearby }
    // Once every paired computer showed up there is nothing left to wait for.
    val complete = settled || inRange.size == saved.size
    return when {
        inRange.isEmpty() || !complete -> ComputerChoice.Wait
        inRange.size == 1 -> ComputerChoice.Connect(inRange.single())
        else -> ComputerChoice.Ask(inRange)
    }
}

/** Paired computers that just came into range, other than the one in use: worth a notice. */
fun newlyNearby(
    before: Set<String>,
    now: Set<String>,
    inUse: String?,
): Set<String> = now - before - setOfNotNull(inUse)
