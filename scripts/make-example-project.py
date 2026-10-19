#!/usr/bin/env python3
"""Writes examples/BandControl-Example.RPP: the smallest project laid out the way BandControl expects.

    Click, Backtrack          sources at the top level
    INPUTS/                   the musicians' inputs: the Inputs tab (levels and gain)
    PA OUT/                   feeds to the venue's console, one hardware output each: the PA tab
    IEM OUT/                  one in-ear mix per musician, a hardware output receiving the sources: the IEM tab

Four songs (regions) at REAPER's default 120 BPM in 4/4, one empty bar apart. A click source on the Click track
plays during every song, so meters have something to show. Deterministic: running it again gives the same file.

Usage: scripts/make-example-project.py
"""

import random
import uuid
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "examples" / "BandControl-Example.RPP"
rng = random.Random(4)

# PEAKCOL values (REAPER's native 0x01BBGGRR), 0 for the theme default.
CLICK, SOURCE, INPUT, PA, IEM = 0x10000C0, 0x1C08040, 0x10060C0, 0x1308040, 0x1C0A030

SOURCES = ["Click", "Backtrack", "Drums", "Bass", "Guitar", "Vocals"]


def mix(*levels: float, muted: tuple[str, ...] = ()) -> list[tuple[str, float, int]]:
    """Receives of an in-ear mix: every source, at these levels."""
    return [(src, level, 1 if src in muted else 0) for src, level in zip(SOURCES, levels)]


def stereo(pair: int) -> str:
    """HWOUT to hardware outputs 2*pair+1 / 2*pair+2."""
    return f"{2 * pair} 0 1 0 0 0 0 -1:U -1"


def mono(output: int) -> str:
    """HWOUT to hardware output `output` (1-based) alone."""
    return f"{1024 + output - 1} 0 1 0 0 0 0 -1:U -1"


# (name, folder: "" | "start" | "end", PEAKCOL, input channel, HWOUT, receives)
TRACKS = [
    ("Click", "", CLICK, None, None, []),
    ("Backtrack", "", SOURCE, None, None, []),
    ("INPUTS", "start", 0, None, None, []),
    ("Drums", "", INPUT, 0, None, []),
    ("Bass", "", INPUT, 1, None, []),
    ("Guitar", "", INPUT, 2, None, []),
    ("Vocals", "end", INPUT, 3, None, []),
    ("PA OUT", "start", 0, None, None, []),
    ("PA Backtrack", "", PA, None, mono(9), [("Backtrack", 1.0, 0)]),
    ("PA Instruments", "", PA, None, mono(10), [("Drums", 1.0, 0), ("Bass", 1.0, 0), ("Guitar", 1.0, 0)]),
    ("PA Vocals", "end", PA, None, mono(11), [("Vocals", 1.0, 0)]),
    ("IEM OUT", "start", 0, None, None, []),
    ("Drummer", "", IEM, None, stereo(0), mix(1.0, 0.7, 1.0, 0.8, 0.5, 0.6)),
    ("Bassist", "", IEM, None, stereo(1), mix(0.8, 0.7, 0.9, 1.0, 0.5, 0.6)),
    ("Guitarist", "", IEM, None, stereo(2), mix(0.6, 0.7, 0.5, 0.6, 1.0, 0.7)),
    ("Singer", "end", IEM, None, stereo(3), mix(0.5, 0.8, 0.4, 0.5, 0.6, 1.0, muted=("Click",))),
]

BPM = 120
BAR = 4 * 60 / BPM
SONGS = [("Spring", 64), ("Summer", 72), ("Autumn", 56), ("Winter", 80)]


def guid() -> str:
    return "{" + str(uuid.UUID(int=rng.getrandbits(128))).upper() + "}"


def main() -> None:
    regions, pos = [], 0.0
    for name, bars in SONGS:
        regions.append((len(regions) + 1, name, pos, pos + bars * BAR))
        pos += (bars + 1) * BAR

    out = [
        '<REAPER_PROJECT 0.1 "7.0" 1760000000',
        "  RIPPLE 0",
        "  AUTOXFADE 129",
        "  CURSOR 0",
        "  LOOP 0",
        "  SAMPLERATE 48000 0 0",
        f"  TEMPO {BPM} 4 4",
    ]
    for rid, name, start, end in regions:
        out.append(f'  MARKER {rid} {start:.6f} "{name}" 1 0 1 B {guid()} 0')
        out.append(f'  MARKER {rid} {end:.6f} "" 1')

    index = {name: i for i, (name, *_) in enumerate(TRACKS)}
    for i, (name, folder, peakcol, channel, hwout, receives) in enumerate(TRACKS):
        track_guid = guid()
        isbus = {"": "0 0", "start": "1 1", "end": "2 -1"}[folder]
        out += [
            f"  <TRACK {track_guid}",
            f'    NAME "{name}"',
            f"    PEAKCOL {peakcol}",
            "    VOLPAN 1 0 -1 -1 1",
            "    MUTESOLO 0 0 0",
            f"    ISBUS {isbus}",
        ]
        if channel is not None:
            # Mono hardware input `channel + 1`, monitored while armed.
            out.append(f"    REC 0 {channel} 1 0 0 0 0 0")
        out += [
            "    NCHAN 2",
            f"    TRACKID {track_guid}",
            # Only hardware outputs reach the speakers and in-ears: nothing goes to the master.
            "    MAINSEND 0 0",
        ]
        out += [f"    AUXRECV {index[src]} 0 {v} 0 {mute} 0 0 0 0 -1:U 0 -1 ''" for src, v, mute in receives]
        if hwout:
            out.append(f"    HWOUT {hwout}")
        if name == "Click":
            for rid, _, start, end in regions:
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
    out.append(">")
    OUT.parent.mkdir(exist_ok=True)
    OUT.write_text("\n".join(out) + "\n")
    print(f"wrote {OUT} ({len(regions)} songs, {len(TRACKS)} tracks)")


if __name__ == "__main__":
    main()
