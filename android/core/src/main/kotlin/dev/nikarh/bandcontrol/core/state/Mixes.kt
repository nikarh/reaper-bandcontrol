package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.core.Commands
import dev.nikarh.bandcontrol.core.FaderLaw
import dev.nikarh.bandcontrol.proto.ClientMessage
import dev.nikarh.bandcontrol.proto.SetReceive
import dev.nikarh.bandcontrol.proto.Track
import kotlin.math.abs

/** One source of a mix: the receive's level and mute, by the source track's name. */
data class SourceLevel(
    val source: String,
    val volume: Float,
    val mute: Boolean,
)

/**
 * The balance of a mix by source track names, so it still applies after REAPER restarts (track ids don't survive
 * that) and to another mix with the same sources. [volume] is the mix's own level, null when not part of it.
 * Sources with the same name are told apart by their order.
 */
data class MixLevels(
    val volume: Float?,
    val sources: List<SourceLevel>,
) {
    /** Same levels and mutes as [other] for every source both have, and the same mix level if both have one. */
    fun sameAs(other: MixLevels): Boolean {
        if (volume != null && other.volume != null && !sameGain(volume, other.volume)) return false
        val theirs = other.sources.byName()
        return sources.byName().all { (key, mine) ->
            val their = theirs[key] ?: return@all true
            mine.mute == their.mute && sameGain(mine.volume, their.volume)
        }
    }
}

/** What happens to a source of the mix that [MixLevels] doesn't mention. */
enum class Missing {
    /** Stays as it is: a preset saved before the source existed. */
    Keep,

    /** Is muted: listening to another mix, where that source isn't heard. */
    Mute,
}

/** The levels of [output] now; [withVolume] includes its own level. */
fun BandState.levelsOf(
    output: Track,
    withVolume: Boolean = true,
): MixLevels =
    MixLevels(
        volume = output.volume.takeIf { withVolume },
        sources = receivesOf(output).map { SourceLevel(it.source.name, it.volume, it.mute) },
    )

/** The commands that make [output] sound like [levels], only for what differs. */
fun BandState.commandsFor(
    output: Track,
    levels: MixLevels,
    missing: Missing = Missing.Keep,
): List<ClientMessage> {
    val commands = mutableListOf<ClientMessage>()
    val volume = levels.volume
    if (volume != null && !sameGain(volume, output.volume)) commands += Commands.trackVolume(output.id, volume)
    val wanted = levels.sources.byName()
    val rows = receivesOf(output)
    for ((key, row) in rows.byName { it.source.name }) {
        val target = wanted[key]
        when {
            target != null -> {
                val setVolume = !sameGain(target.volume, row.volume)
                val setMute = target.mute != row.mute
                if (setVolume || setMute) {
                    commands +=
                        ClientMessage(
                            set_receive =
                                SetReceive(
                                    track = output.id,
                                    index = row.index,
                                    src = row.source.id,
                                    volume = target.volume.takeIf { setVolume },
                                    mute = target.mute.takeIf { setMute },
                                ),
                        )
                }
            }

            missing == Missing.Mute && !row.mute -> {
                commands += Commands.receiveMute(output.id, row.index, row.source.id, true)
            }
        }
    }
    return commands
}

/** Differences below this are rounding (REAPER keeps doubles, the protocol floats), not a change. */
private const val SAME_DB = 0.05f

private fun sameGain(
    a: Float,
    b: Float,
): Boolean {
    if (a == b) return true
    val da = FaderLaw.gainToDb(a)
    val db = FaderLaw.gainToDb(b)
    // Below the fader's range everything is silence.
    if (da < MIN_DB && db < MIN_DB) return true
    return abs(da - db) < SAME_DB
}

private const val MIN_DB = -100f

/** Keyed by name and the how-manieth source of that name it is. */
private fun List<SourceLevel>.byName(): Map<Pair<String, Int>, SourceLevel> = byName { it.source }

private fun <T> List<T>.byName(name: (T) -> String): Map<Pair<String, Int>, T> {
    val seen = mutableMapOf<String, Int>()
    return associateBy { item ->
        val n = name(item)
        val index = seen.getOrDefault(n, 0)
        seen[n] = index + 1
        n to index
    }
}

/** The mix (output track) called [name]. */
fun BandState.mix(name: String): Track? = tracks.find { it.output && it.name == name }

/**
 * What makes the [Listening.mine] mix sound like the [Listening.to] mix: the same balance of the same sources, the
 * others muted, the mix level left alone (it suits the listener's own ears). Empty when they already match, null
 * when a mix is gone.
 */
fun BandState.listenCommands(listening: Listening): List<ClientMessage>? {
    val mine = mix(listening.mine) ?: return null
    val to = mix(listening.to) ?: return null
    return commandsFor(mine, levelsOf(to, withVolume = false), Missing.Mute)
}

/** The same change for the [Listening.mine] mix as [message] makes to the [Listening.to] mix, if it is one. */
fun BandState.mirror(
    message: ClientMessage,
    listening: Listening,
): ClientMessage? {
    val set = message.set_receive ?: return null
    val to = mix(listening.to)?.takeIf { it.id == set.track } ?: return null
    val mine = mix(listening.mine) ?: return null
    if (to.receives.getOrNull(set.index)?.src != set.src) return null
    val row = receivesOf(mine).find { it.source.id == set.src } ?: return null
    return ClientMessage(set_receive = set.copy(track = mine.id, index = row.index))
}
