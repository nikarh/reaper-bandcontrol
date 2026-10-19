//! Reads the parts of a REAPER project file (`.RPP`) the protocol needs into a [`FakeDaw`], so the simulator can
//! serve a real project without REAPER.

use crate::{
    fake::{FakeDaw, FakeReceive, FakeTrack},
    model::{Marker, Region, TrackId, parse_region_order},
};

/// REAPER sets this bit on colors that are not the theme default.
const CUSTOM_COLOR: u32 = 0x0100_0000;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RppError {
    #[error("not a REAPER project")]
    NotAProject,
    #[error("line {0}: unbalanced block")]
    Unbalanced(usize),
}

/// Splits a line into tokens; tokens starting with `"`, `'` or `` ` `` extend to the matching quote.
fn tokens(line: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'));
        let (token, next) = match quote {
            Some(q) => match rest[1..].find(q) {
                Some(end) => (&rest[1..=end], &rest[end + 2..]),
                None => (&rest[1..], ""),
            },
            None => rest.split_at(rest.find(char::is_whitespace).unwrap_or(rest.len())),
        };
        out.push(token);
        rest = next.trim_start();
    }
    out
}

fn color(native: Option<&str>, windows_order: bool) -> Option<u32> {
    let native = u32::try_from(native?.parse::<i64>().ok()?).ok()?;
    // REAPER reports the flag without a color as no color.
    if native & CUSTOM_COLOR == 0 || native == CUSTOM_COLOR {
        return None;
    }
    let c = native & 0x00ff_ffff;
    Some(if windows_order {
        ((c & 0xff) << 16) | (c & 0xff00) | (c >> 16)
    } else {
        c
    })
}

fn number<T: std::str::FromStr>(token: Option<&&str>) -> Option<T> {
    token?.parse().ok()
}

struct TrackBuilder {
    track: FakeTrack,
    folder: (i32, i32),
}

impl TrackBuilder {
    fn attribute(&mut self, t: &[&str]) {
        match t.first().copied().unwrap_or_default() {
            "NAME" => t.get(1).copied().unwrap_or_default().clone_into(&mut self.track.name),
            "PEAKCOL" => self.track.color = color(t.get(1).copied(), true),
            "VOLPAN" => self.track.volume = number(t.get(1)).unwrap_or(1.0),
            "ISBUS" => self.folder = (number(t.get(1)).unwrap_or(0), number(t.get(2)).unwrap_or(0)),
            "HWOUT" => self.track.hw_out = true,
            "AUXRECV" => {
                if let Some(src) = number::<TrackId>(t.get(1)) {
                    self.track.receives.push(FakeReceive {
                        src: src + 1,
                        volume: number(t.get(3)).unwrap_or(1.0),
                        mute: t.get(5) == Some(&"1"),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Region starts waiting for their end marker.
type RegionStart = (u32, String, f64, Option<u32>);

/// `MARKER id pos name flags color ...`; a region is a start line with the region flag and an end line.
fn marker(daw: &mut FakeDaw, region_starts: &mut Vec<RegionStart>, t: &[&str]) {
    let (Some(id), Some(pos)) = (number::<u32>(t.get(1)), number::<f64>(t.get(2))) else {
        return;
    };
    let name = t.get(3).copied().unwrap_or_default().to_owned();
    let is_region = number::<u32>(t.get(4)).unwrap_or(0) & 1 == 1;
    let c = color(t.get(5).copied(), false);
    if !is_region {
        daw.markers.push(Marker {
            id,
            name,
            pos,
            color: c,
        });
    } else if let Some(i) = region_starts.iter().position(|r| r.0 == id) {
        let (id, name, start, color) = region_starts.remove(i);
        daw.regions.push(Region {
            id,
            name,
            start,
            end: pos,
            color,
        });
    } else {
        region_starts.push((id, name, pos, c));
    }
}

/// Parses a project. Track ids are 1-based track numbers.
///
/// # Errors
/// When the text is not a REAPER project or its blocks don't nest.
pub fn parse(text: &str) -> Result<FakeDaw, RppError> {
    let mut daw = FakeDaw::default();
    // Open blocks, innermost last.
    let mut blocks: Vec<String> = vec![];
    let mut current: Option<TrackBuilder> = None;
    let mut folders: Vec<TrackId> = vec![];
    let mut region_starts: Vec<RegionStart> = vec![];

    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        let t = tokens(line);
        let Some(&first) = t.first() else {
            continue;
        };
        if blocks.is_empty() && first != "<REAPER_PROJECT" {
            return Err(RppError::NotAProject);
        }
        if let Some(block) = first.strip_prefix('<') {
            if block == "TRACK" && blocks.len() == 1 {
                let id = TrackId::try_from(daw.tracks.len() + 1).unwrap_or(TrackId::MAX);
                current = Some(TrackBuilder {
                    track: FakeTrack {
                        id,
                        name: String::new(),
                        color: None,
                        volume: 1.0,
                        hw_out: false,
                        receives: vec![],
                        parent: folders.last().copied(),
                    },
                    folder: (0, 0),
                });
            }
            blocks.push(block.to_owned());
            continue;
        }
        if first == ">" {
            let block = blocks.pop().ok_or(RppError::Unbalanced(n + 1))?;
            if block == "TRACK"
                && blocks.len() == 1
                && let Some(TrackBuilder { track, folder }) = current.take()
            {
                let mut track = track;
                if track.name.is_empty() {
                    // REAPER shows unnamed tracks by number.
                    track.name = format!("Track {}", track.id);
                }
                if folder.0 == 1 {
                    folders.push(track.id);
                }
                for _ in 0..(-folder.1).max(0) {
                    folders.pop();
                }
                daw.tracks.push(track);
            }
            continue;
        }

        match (blocks.as_slice(), first) {
            ([_, track], _) if track == "TRACK" => {
                if let Some(b) = current.as_mut() {
                    b.attribute(&t);
                }
            }
            ([_], "MARKER") => marker(&mut daw, &mut region_starts, &t),
            ([.., bandui], "REGIONS") if bandui == "BANDUI" => {
                daw.region_order = parse_region_order(t.get(1).copied().unwrap_or_default());
            }
            _ => {}
        }
    }
    if !blocks.is_empty() {
        return Err(RppError::Unbalanced(text.lines().count()));
    }
    if daw.tracks.is_empty() && daw.regions.is_empty() && text.trim().is_empty() {
        return Err(RppError::NotAProject);
    }
    daw.regions.sort_by_key(|r| r.id);
    daw.markers.sort_by_key(|m| m.id);
    Ok(daw)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::convert::seconds_to_ms;

    #[test]
    fn tokenizes_quotes() {
        assert_eq!(
            tokens(r#"MARKER 3 83.4 "Paper Lions" 1 '' `a "b"` x"#),
            vec!["MARKER", "3", "83.4", "Paper Lions", "1", "", r#"a "b""#, "x"]
        );
    }

    #[test]
    fn rejects_other_files() {
        assert_eq!(parse("hello").unwrap_err(), RppError::NotAProject);
        assert_eq!(
            parse("<REAPER_PROJECT\n<TRACK\n>").unwrap_err(),
            RppError::Unbalanced(3)
        );
    }

    /// Colors in project files are rounded slightly differently from what REAPER reports.
    fn close(a: Option<u32>, b: Option<u32>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => (0..3).all(|i| ((a >> (i * 8)) & 0xff).abs_diff((b >> (i * 8)) & 0xff) <= 1),
            (a, b) => a == b,
        }
    }

    #[test]
    fn mother_project_matches_the_fake() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/Mother Project.RPP");
        let parsed = parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        let fake = FakeDaw::mother_project();

        assert_eq!(parsed.tracks.len(), fake.tracks.len());
        for (p, f) in parsed.tracks.iter().zip(&fake.tracks) {
            assert!(close(p.color, f.color), "{p:?} {f:?}");
            assert_eq!(
                FakeTrack {
                    color: f.color,
                    ..p.clone()
                },
                *f
            );
        }
        let ms = |s: f64| seconds_to_ms(s);
        let regions = |d: &FakeDaw| {
            d.regions
                .iter()
                .map(|r| (r.id, r.name.clone(), ms(r.start), ms(r.end)))
                .collect::<Vec<_>>()
        };
        assert_eq!(regions(&parsed), regions(&fake));
        let markers = |d: &FakeDaw| {
            d.markers
                .iter()
                .map(|m| (m.id, m.name.clone(), ms(m.pos)))
                .collect::<Vec<_>>()
        };
        assert_eq!(markers(&parsed), markers(&fake));
        for (p, f) in parsed.regions.iter().zip(&fake.regions) {
            assert!(close(p.color, f.color), "{p:?} {f:?}");
        }
        for (p, f) in parsed.markers.iter().zip(&fake.markers) {
            assert!(close(p.color, f.color), "{p:?} {f:?}");
        }
        assert_eq!(parsed.region_order, fake.region_order);
    }

    #[test]
    fn example_project_layout() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/BandControl-Example.RPP");
        let daw = parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        let names = |parent: Option<TrackId>| {
            daw.tracks
                .iter()
                .filter(|t| t.parent == parent)
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(None), ["Click", "Backtrack", "INPUTS", "FOH OUT", "IEM OUT"]);
        assert_eq!(names(Some(3)), ["Drums", "Bass", "Guitar", "Vocals"]);
        assert_eq!(names(Some(8)), ["FOH Backtrack", "FOH Instruments", "FOH Vocals"]);
        assert_eq!(names(Some(12)), ["Drummer", "Bassist", "Guitarist", "Singer"]);
        for track in daw
            .tracks
            .iter()
            .filter(|t| t.parent == Some(8) || t.parent == Some(12))
        {
            assert!(track.hw_out && !track.receives.is_empty(), "{track:?}");
        }
        // Every in-ear mix hears every source.
        assert!(
            daw.tracks
                .iter()
                .filter(|t| t.parent == Some(12))
                .all(|t| t.receives.len() == 6)
        );
        let ms = |s: f64| seconds_to_ms(s);
        let songs: Vec<_> = daw
            .regions
            .iter()
            .map(|r| (r.name.as_str(), ms(r.start), ms(r.end)))
            .collect();
        // 120 BPM in 4/4: a bar is 2 s, one empty bar between songs.
        assert_eq!(
            songs,
            [
                ("Spring", 0, 128_000),
                ("Summer", 130_000, 274_000),
                ("Autumn", 276_000, 388_000),
                ("Winter", 390_000, 550_000)
            ]
        );
        assert_eq!(daw.markers.len(), 0);
    }

    #[test]
    fn band_project_folders() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/Band Project.RPP");
        let daw = parse(&std::fs::read_to_string(path).unwrap()).unwrap();
        let track = |id: TrackId| &daw.tracks[usize::try_from(id - 1).unwrap()];
        assert_eq!(daw.tracks.len(), 31);
        assert_eq!(track(5).name, "INPUTS");
        assert_eq!(track(6).parent, Some(5));
        // The last track of a folder closes it.
        assert_eq!(track(12).parent, Some(5));
        assert_eq!(track(13).parent, None);
        assert_eq!(track(18).name, "PA Backtrack");
        assert_eq!(track(18).parent, Some(17));
        assert!(track(18).hw_out);
        assert_eq!(track(18).receives[0].src, 3);
        assert_eq!(track(27).name, "Drums");
        assert_eq!(track(27).parent, Some(26));
        assert_eq!(track(27).receives.len(), 10);
        assert!(track(27).receives[2].mute);
        assert_eq!(track(31).parent, Some(26));
        assert_eq!(daw.regions.len(), 17);
        assert_eq!(daw.regions[2].name, "Paper Lions");
        assert_eq!(daw.markers.len(), 4);
        assert_eq!(daw.markers[3].name, "!1016");
        assert_eq!(
            daw.region_order[2],
            crate::model::RegionOrder { id: 6, disabled: false }
        );
        assert_eq!(
            daw.region_order[15],
            crate::model::RegionOrder { id: 16, disabled: true }
        );
    }
}
