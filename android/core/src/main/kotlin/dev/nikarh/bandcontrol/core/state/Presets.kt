package dev.nikarh.bandcontrol.core.state

import org.json.JSONArray
import org.json.JSONObject

/** A saved balance of one mix. */
data class MixPreset(
    val name: String,
    val levels: MixLevels,
)

/**
 * The presets of one mix. [active] is the preset last loaded or saved ([UNSAVED] for [unsaved]); [unsaved] keeps the
 * mix as it was before a preset replaced changes that weren't saved, so trying a preset never loses them.
 */
data class MixPresets(
    val presets: List<MixPreset> = emptyList(),
    val active: String? = null,
    val unsaved: MixLevels? = null,
) {
    fun preset(name: String) = presets.find { it.name == name }

    /** The levels [active] stands for. */
    fun activeLevels(): MixLevels? = if (active == UNSAVED) unsaved else active?.let { preset(it)?.levels }

    /** The mix differs from what was last loaded or saved. */
    fun modified(current: MixLevels): Boolean = activeLevels()?.let { !it.sameAs(current) } ?: false

    /** Keeps [current] as [unsaved] if loading something else would lose it: it matches nothing saved. */
    private fun keeping(current: MixLevels): MixPresets {
        val saved = presets.any { it.levels.sameAs(current) } || unsaved?.sameAs(current) == true
        return if (saved) this else copy(unsaved = current)
    }

    /** Loads preset [name] (or [UNSAVED]) over [current]: the presets afterwards and the levels to apply. */
    fun load(
        name: String,
        current: MixLevels,
    ): Pair<MixPresets, MixLevels>? {
        val levels = (if (name == UNSAVED) unsaved else preset(name)?.levels) ?: return null
        return keeping(current).copy(active = name) to levels
    }

    /** Saves [levels] as [name], replacing a preset of that name. */
    fun save(
        name: String,
        levels: MixLevels,
    ): MixPresets {
        val preset = MixPreset(name, levels)
        val replaced = presets.any { it.name == name }
        return copy(
            presets = if (replaced) presets.map { if (it.name == name) preset else it } else presets + preset,
            active = name,
            // Saved now: nothing to keep.
            unsaved = unsaved?.takeUnless { it.sameAs(levels) },
        )
    }

    fun rename(
        from: String,
        to: String,
    ): MixPresets {
        if (from == to || preset(from) == null) return this
        val kept = presets.filter { it.name != to }
        return copy(
            presets = kept.map { if (it.name == from) it.copy(name = to) else it },
            active = if (active == from) to else active.takeIf { it != to },
        )
    }

    fun delete(name: String): MixPresets = copy(presets = presets.filter { it.name != name }, active = active.takeIf { it != name })

    /** A name for the next preset that isn't taken. */
    fun newName(): String = generateSequence(presets.size + 1) { it + 1 }.map { "Preset $it" }.first { preset(it) == null }

    companion object {
        /** [active] when the unsaved changes were restored. */
        const val UNSAVED = ""
    }
}

/**
 * Listening to [to] in the [mine] mix, to hear what a bandmate hears while helping with their mix. [before] is
 * the [mine] mix as it was, to go back to.
 */
data class Listening(
    val mine: String,
    val to: String,
    val before: MixLevels,
)

/** Everything a phone keeps about the mixes of one computer, by mix (output track) name. */
data class Presets(
    val mixes: Map<String, MixPresets> = emptyMap(),
    /** The phone owner's own mix. */
    val mine: String? = null,
    val listening: Listening? = null,
) {
    fun of(mix: String) = mixes[mix] ?: MixPresets()

    fun with(
        mix: String,
        presets: MixPresets,
    ) = copy(mixes = mixes + (mix to presets))

    fun toJson(): String =
        JSONObject()
            .put(
                "mixes",
                JSONObject().also { o ->
                    for ((mix, p) in mixes) {
                        o.put(
                            mix,
                            JSONObject()
                                .put(
                                    "presets",
                                    JSONArray(p.presets.map { JSONObject().put("name", it.name).put("levels", it.levels.toJson()) }),
                                ).putOpt("active", p.active)
                                .putOpt("unsaved", p.unsaved?.toJson()),
                        )
                    }
                },
            ).putOpt("mine", mine)
            .putOpt(
                "listening",
                listening?.let { JSONObject().put("mine", it.mine).put("to", it.to).put("before", it.before.toJson()) },
            ).toString()

    companion object {
        /** The presets stored by [toJson], empty for anything else. */
        fun fromJson(text: String?): Presets {
            val o = text?.let { runCatching { JSONObject(it) }.getOrNull() } ?: return Presets()
            return runCatching {
                val mixes = o.optJSONObject("mixes") ?: JSONObject()
                Presets(
                    mixes =
                        mixes.keys().asSequence().associateWith { mix ->
                            val m = mixes.getJSONObject(mix)
                            val list = m.optJSONArray("presets") ?: JSONArray()
                            MixPresets(
                                presets =
                                    (0 until list.length()).map { i ->
                                        val p = list.getJSONObject(i)
                                        MixPreset(p.getString("name"), levelsFromJson(p.getJSONObject("levels")))
                                    },
                                active = if (m.has("active")) m.getString("active") else null,
                                unsaved = m.optJSONObject("unsaved")?.let(::levelsFromJson),
                            )
                        },
                    mine = if (o.has("mine")) o.getString("mine") else null,
                    listening =
                        o.optJSONObject("listening")?.let {
                            Listening(it.getString("mine"), it.getString("to"), levelsFromJson(it.getJSONObject("before")))
                        },
                )
            }.getOrDefault(Presets())
        }
    }
}

private fun MixLevels.toJson(): JSONObject =
    JSONObject()
        .putOpt("volume", volume?.toDouble())
        .put(
            "sources",
            JSONArray(sources.map { JSONObject().put("source", it.source).put("volume", it.volume.toDouble()).put("mute", it.mute) }),
        )

private fun levelsFromJson(o: JSONObject): MixLevels {
    val sources = o.getJSONArray("sources")
    return MixLevels(
        volume = if (o.has("volume")) o.getDouble("volume").toFloat() else null,
        sources =
            (0 until sources.length()).map { i ->
                val s = sources.getJSONObject(i)
                SourceLevel(s.getString("source"), s.getDouble("volume").toFloat(), s.getBoolean("mute"))
            },
    )
}
