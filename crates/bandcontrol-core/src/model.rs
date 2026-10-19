//! Domain model, independent of the wire format.

use std::collections::HashSet;

pub type TrackId = u32;

/// Everything that changes rarely. Compared as a whole to detect changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Structure {
    pub tracks: Vec<TrackInfo>,
    pub regions: Vec<Region>,
    pub markers: Vec<Marker>,
    pub region_order: Vec<RegionOrder>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub id: TrackId,
    /// 1-based position in the track list.
    pub number: u32,
    pub name: String,
    /// 0xRRGGBB.
    pub color: Option<u32>,
    /// Has a hardware output and at least one receive.
    pub output: bool,
    pub receives: Vec<ReceiveInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiveInfo {
    pub src: TrackId,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub id: u32,
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub color: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub id: u32,
    pub name: String,
    pub pos: f64,
    pub color: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionOrder {
    pub id: u32,
    pub disabled: bool,
}

/// Gains, aligned with [`Structure::tracks`] and their receives.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mix {
    pub tracks: Vec<TrackMix>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackMix {
    pub volume: f32,
    pub receives: Vec<ReceiveMix>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReceiveMix {
    pub volume: f32,
    pub mute: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TransportState {
    pub play: PlayState,
    /// Seconds.
    pub position: f64,
    pub repeat: bool,
    pub recording: bool,
}

impl Structure {
    /// Keeps only output tracks and the tracks feeding them; receives are only kept on output tracks.
    #[must_use]
    pub fn relevant(mut self) -> Self {
        let sources: HashSet<TrackId> = self
            .tracks
            .iter()
            .filter(|t| t.output)
            .flat_map(|t| t.receives.iter().map(|r| r.src))
            .collect();
        self.tracks.retain(|t| t.output || sources.contains(&t.id));
        for t in &mut self.tracks {
            if !t.output {
                t.receives.clear();
            }
        }
        self
    }
}

/// Region order is stored in project ext state as `BANDUI/regions`, compatible with reaper-remote-bandui:
/// comma separated `<enabled 0|1><region id>`.
#[must_use]
pub fn parse_region_order(value: &str) -> Vec<RegionOrder> {
    value
        .split(',')
        .filter_map(|item| {
            let (flag, id) = item.trim().split_at_checked(1)?;
            Some(RegionOrder {
                id: id.parse().ok()?,
                disabled: flag == "0",
            })
        })
        .collect()
}

#[must_use]
pub fn format_region_order(order: &[RegionOrder]) -> String {
    order
        .iter()
        .map(|o| format!("{}{}", if o.disabled { '0' } else { '1' }, o.id))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn region_order_roundtrip() {
        let order = parse_region_order("11,14,03,12");
        assert_eq!(
            order,
            vec![
                RegionOrder { id: 1, disabled: false },
                RegionOrder { id: 4, disabled: false },
                RegionOrder { id: 3, disabled: true },
                RegionOrder { id: 2, disabled: false },
            ]
        );
        assert_eq!(format_region_order(&order), "11,14,03,12");
        assert_eq!(parse_region_order(""), vec![]);
        assert_eq!(
            parse_region_order("1x,,12"),
            vec![RegionOrder { id: 2, disabled: false }]
        );
    }

    #[test]
    fn relevant_keeps_outputs_and_their_sources() {
        let track = |id, output, receives: &[TrackId]| TrackInfo {
            id,
            number: id,
            name: format!("t{id}"),
            color: None,
            output,
            receives: receives.iter().map(|&src| ReceiveInfo { src }).collect(),
        };
        let s = Structure {
            tracks: vec![
                track(1, false, &[]),
                track(2, false, &[]),
                track(3, false, &[1]),
                track(4, true, &[2]),
            ],
            ..Structure::default()
        }
        .relevant();
        assert_eq!(s.tracks, vec![track(2, false, &[]), track(4, true, &[2])]);
    }
}
