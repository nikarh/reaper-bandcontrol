package dev.nikarh.bandcontrol.core.state

import dev.nikarh.bandcontrol.proto.Track

/**
 * Tracks by role, from the project's folder layout:
 * - inputs: direct children of a top level folder named like "Inputs",
 * - PA outputs: direct children of a top level folder with "PA" in its name,
 * - IEM outputs: output tracks (hardware output and receives) outside the PA folders; when some of them are inside a
 *   folder named like "IEM" or "Monitor", only those.
 */
data class TrackGroups(
    val inputs: List<Track> = emptyList(),
    val iemOutputs: List<Track> = emptyList(),
    val paOutputs: List<Track> = emptyList(),
)

private val INPUT_FOLDER = Regex("""\binputs?\b""", RegexOption.IGNORE_CASE)
private val PA_FOLDER = Regex("""\bPA\b""", RegexOption.IGNORE_CASE)
private val IEM_FOLDER = Regex("""\bIEM\b|monitor""", RegexOption.IGNORE_CASE)

fun BandState.groups(): TrackGroups {
    val byId = tracks.associateBy { it.id }

    fun ancestors(track: Track): List<Track> {
        val chain = mutableListOf<Track>()
        var parent = track.parent?.let(byId::get)
        while (parent != null && parent !in chain) {
            chain += parent
            parent = parent.parent?.let(byId::get)
        }
        return chain
    }

    val folders = tracks.mapNotNull { it.parent }.toSet()
    val topFolders = tracks.filter { it.id in folders && it.parent == null }

    fun childrenOf(pattern: Regex): List<Track> {
        val ids = topFolders.filter { pattern.containsMatchIn(it.name) }.map { it.id }.toSet()
        return tracks.filter { it.parent in ids }
    }

    val paFolders = topFolders.filter { PA_FOLDER.containsMatchIn(it.name) }.toSet()
    val outputs = tracks.filter { t -> t.output && ancestors(t).none { it in paFolders } }
    val monitored = outputs.filter { t -> ancestors(t).any { IEM_FOLDER.containsMatchIn(it.name) } }
    return TrackGroups(
        inputs = childrenOf(INPUT_FOLDER),
        iemOutputs = monitored.ifEmpty { outputs },
        paOutputs = childrenOf(PA_FOLDER),
    )
}
