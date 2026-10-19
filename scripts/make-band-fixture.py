#!/usr/bin/env python3
"""Writes tests/fixtures/Band Project.RPP: a band's live project (backtracks, inputs, vocals processed for the
in-ears, PA outputs and one in-ear mix per band member) with invented songs and tempos.

Deterministic: running it again produces the same file. Third party plugins are left out so the project opens in
any REAPER; a click source on the first track plays during every song, so meters have something to show.

Usage: scripts/make-band-fixture.py
"""

import random
import uuid
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "Band Project.RPP"
rng = random.Random(20261019)

# Folders and tracks, `/` marks a folder. Receives name their source track: (source, volume, mute).
CLICK, BACKTRACK, INPUT, IEM_VOX, PA, IEM = 16777216, 16576, 16777382, 29367552, 16826381, 16842751

MIX = ["Click", "Backtrack", "Vocal Backtrack", "Drums input", "Bass input", "L GTR input", "R GTR input",
       "IEM VOX", "IEM VOX BASS", "IEM VOX R GTR"]


def mix(*levels: float, muted: tuple[str, ...] = ()) -> list[tuple[str, float, int]]:
    return [(src, level, 1 if src in muted else 0) for src, level in zip(MIX, levels)]


# (name, folder: "" | "start" | "end", volume, PEAKCOL, HWOUT, receives)
TRACKS = [
    ("Click", "", 1.0, CLICK, None, []),
    ("BACKTRACKS", "start", 1.0, 0, None, []),
    ("Backtrack", "", 1.0, BACKTRACK, None, []),
    ("Vocal Backtrack", "end", 1.0, BACKTRACK, None, []),
    ("INPUTS", "start", 1.0, 0, None, []),
    ("Drums input", "", 1.0, 25198847, None, []),
    ("Bass input", "", 1.0, INPUT, None, []),
    ("L GTR input", "", 1.995262, INPUT, None, []),
    ("R GTR input", "", 1.995262, INPUT, None, []),
    ("VOX input", "", 1.0, INPUT, None, []),
    ("VOX BASS input", "", 1.0, INPUT, None, []),
    ("VOX R GTR input", "end", 1.0, INPUT, None, []),
    ("IEM VOCALS", "start", 1.0, 0, None, []),
    ("IEM VOX", "", 1.0, IEM_VOX, None, [("VOX input", 1.0, 0)]),
    ("IEM VOX BASS", "", 1.0, IEM_VOX, None, [("VOX BASS input", 1.0, 0)]),
    ("IEM VOX R GTR", "end", 1.0, IEM_VOX, None, [("VOX R GTR input", 1.0, 0)]),
    ("PA OUT", "start", 1.0, 0, None, []),
    ("PA Backtrack", "", 0.562341, PA, "1037 0 1 0 0 0 0 -1:U -1", [("Backtrack", 1.0, 0)]),
    ("PA VOX", "", 0.398107, PA, "1040 0 1 0 0 0 0 -1:U -1", [("VOX input", 1.0, 0)]),
    ("PA VOX BACK", "", 0.398107, PA, "1043 0 1 0 0 0 0 -1:U -1", [("Vocal Backtrack", 0.5259, 0)]),
    ("PA VOX BASS", "", 0.398107, PA, "1041 0 1 0 0 0 0 -1:U -1", [("VOX BASS input", 1.0, 0)]),
    ("PA VOX R GTR", "", 0.398107, PA, "1042 0 1 0 0 0 0 -1:U -1", [("VOX R GTR input", 1.0, 0)]),
    ("PA Bass", "", 1.0, PA, "1039 0 1 0 0 0 0 -1:U -1", [("Bass input", 1.0, 0)]),
    ("PA L GTR", "", 0.101501, PA, "1036 0 1 0 0 0 0 -1:U -1", [("L GTR input", 1.0, 0)]),
    ("PA R GTR", "end", 0.707946, PA, "1038 0 1 0 0 0 0 -1:U -1", [("R GTR input", 1.0, 0)]),
    ("IEM OUT", "start", 1.0, 0, None, []),
    ("Drums", "", 0.251189, IEM, "0 0 1 0 0 0 0 -1:U -1",
     mix(1.7783, 0.7943, 0.7943, 0.6683, 1.7783, 1.2589, 1.2589, 0.8913, 0.4467, 0.4467, muted=("Vocal Backtrack",))),
    ("Left guitar", "", 0.316228, IEM, "4 0 1 0 0 0 0 -1:U -1",
     mix(1.0, 0.7917, 0.631, 0.429, 1.9953, 0.5012, 0.5012, 0.5623, 0.5012, 0.5012)),
    ("Right guitar", "", 1.0, IEM, "8 0 1 0 0 0 0 -1:U -1",
     mix(0.5623, 0.4467, 0.3981, 0.2265, 0.5012, 0.7079, 1.6788, 0.1596, 0.3515, 0.7943)),
    ("Bass", "", 0.316228, IEM, "6 0 1 0 0 0 0 -1:U -1",
     mix(1.5849, 1.0, 0.5623, 0.5012, 0.8049, 1.0, 1.0, 0.5623, 1.0, 0.5623)),
    ("Vocals", "end", 0.316228, IEM, "2 0 1 0 0 0 0 -1:U -1",
     mix(1.7783, 0.3628, 1.4125, 0.5544, 0.75, 0.7079, 0.7079, 1.5565, 1.0, 0.631, muted=("Click",))),
]

SONGS = [
    "Glass Harbour", "Paper Lions", "Night Shift", "Copper Sky", "Static Bloom", "Low Tide", "Wildfire Choir",
    "Broken Compass", "Velvet Machine", "Hollow Ground", "Satellite Hearts", "Northbound",
]
# Interludes and cues by position in the set, real projects have many of them.
INTERLUDES = {0: "Intro tape", 3: "Interlude A", 6: "Interlude B", 10: "Interlude C", 16: "Outro tape"}
# Real projects have very few markers: song parts in a song or two, action markers.
SECTIONS = {"Paper Lions": [(8, "Verse"), (24, "Chorus"), (-8, "Outro")]}
ACTIONS = {"Low Tide": "!1016"}  # REAPER action marker: stop at the end of the song
# Native region colors (0x01RRGGBB) or the theme default.
COLORS = [0, 0x018B8727, 0x013C1D8B, 0x01A9EAD2, 0x01C04040, 0x012080C0, 0, 0x0160A030]


def guid() -> str:
    return "{" + str(uuid.UUID(int=rng.getrandbits(128))).upper() + "}"


def main() -> None:
    regions, markers, tempos = [], [], []
    pos, marker_id = 2.0, 1
    for index in range(len(SONGS) + len(INTERLUDES)):
        name = INTERLUDES.get(index) or SONGS[index - sum(1 for i in INTERLUDES if i < index)]
        interlude = index in INTERLUDES
        bpm = rng.choice(range(84, 184, 2))
        bar = 4 * 60 / bpm
        bars = rng.randint(8, 16) if interlude else rng.randint(56, 120)
        start, end = pos, pos + bars * bar
        regions.append((len(regions) + 1, name, start, end, rng.choice(COLORS)))
        tempos.append((start, bpm))
        for at, section in SECTIONS.get(name, []):
            markers.append((marker_id, section, start + (at if at >= 0 else bars + at) * bar))
            marker_id += 1
        if name in ACTIONS:
            markers.append((marker_id, ACTIONS[name], end - bar))
            marker_id += 1
        # Songs follow each other directly or after a short talk.
        pos = end + rng.choice([0, 0, 4.0, 9.5])

    order = list(range(1, len(regions) + 1))
    order[2], order[5] = order[5], order[2]
    disabled = {order[-2]}

    out = [
        '<REAPER_PROJECT 0.1 "7.11" 1760000000',
        "  RIPPLE 0",
        "  AUTOXFADE 129",
        "  CURSOR 0",
        "  LOOP 0",
        "  SAMPLERATE 48000 0 0",
        f"  TEMPO {tempos[0][1]} 4 4",
        "  <TEMPOENVEX",
        "    ACT 1 -1",
        "    VIS 1 0 1",
        "    DEFSHAPE 1 -1 -1",
        *(f"    PT {t:.6f} {bpm:.6f} 1" for t, bpm in [(0.0, tempos[0][1])] + tempos),
        "  >",
    ]
    for rid, name, start, end, color in regions:
        out.append(f'  MARKER {rid} {start:.6f} "{name}" 1 {color} 1 B {guid()} 0')
        out.append(f'  MARKER {rid} {end:.6f} "" 1')
    for mid, name, at in markers:
        out.append(f'  MARKER {mid} {at:.6f} "{name}" 0 0 1 B {guid()} 0')

    index = {name: i for i, (name, *_) in enumerate(TRACKS)}
    for i, (name, folder, vol, peakcol, hwout, receives) in enumerate(TRACKS):
        track_guid = guid()
        isbus = {"": "0 0", "start": "1 1", "end": "2 -1"}[folder]
        out += [
            f"  <TRACK {track_guid}",
            f'    NAME "{name}"',
            f"    PEAKCOL {peakcol}",
            f"    VOLPAN {vol} 0 -1 -1 1",
            "    MUTESOLO 0 0 0",
            f"    ISBUS {isbus}",
            "    NCHAN 2",
            f"    TRACKID {track_guid}",
            "    MAINSEND 0 0",
        ]
        out += [f"    AUXRECV {index[src]} 0 {v} 0 {mute} 0 0 0 0 -1:U 0 -1 ''" for src, v, mute in receives]
        if hwout:
            out.append(f"    HWOUT {hwout}")
        if i == 0:
            for rid, _, start, end, _ in regions:
                out += [
                    "    <ITEM",
                    f"      POSITION {start:.6f}",
                    f"      LENGTH {end - start:.6f}",
                    "      LOOP 1",
                    f"      IGUID {guid()}",
                    f'      NAME "Click {rid}"',
                    "      VOLPAN 1 0 1 -1",
                    "      <SOURCE CLICK",
                    "        AUTO 1",
                    "        BPI 4",
                    "        VOL 1 1",
                    "        FREQ 800 1600 1",
                    "        BEATLEN 4",
                    '        SAMPLES "" ""',
                    "      >",
                    "    >",
                ]
        out.append("  >")

    regions_state = ",".join(f"{0 if r in disabled else 1}{r}" for r in order)
    out += ["  <EXTSTATE", "    <BANDUI", f"      REGIONS {regions_state}", "    >", "  >", ">"]
    OUT.write_text("\n".join(out) + "\n")
    print(f"wrote {OUT} ({len(regions)} regions, {len(markers)} markers, {len(TRACKS)} tracks)")


if __name__ == "__main__":
    main()
