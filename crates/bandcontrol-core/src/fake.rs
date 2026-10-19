//! In-memory DAW used by tests and the `bandctl sim` simulator.

use std::{collections::HashMap, time::Duration};

use crate::{
    daw::{Command, CommandError, Daw},
    model::{
        Marker, Mix, PlayState, ReceiveInfo, ReceiveMix, Region, RegionOrder, Structure, TrackId, TrackInfo, TrackMix,
        TransportState,
    },
};

#[derive(Debug, Clone, PartialEq)]
pub struct FakeTrack {
    pub id: TrackId,
    pub name: String,
    pub color: Option<u32>,
    pub volume: f32,
    pub hw_out: bool,
    pub receives: Vec<FakeReceive>,
    pub parent: Option<TrackId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FakeReceive {
    pub src: TrackId,
    pub volume: f32,
    pub mute: bool,
}

#[derive(Debug, Clone, Default)]
pub struct FakeDaw {
    pub tracks: Vec<FakeTrack>,
    pub regions: Vec<Region>,
    pub markers: Vec<Marker>,
    pub region_order: Vec<RegionOrder>,
    pub transport: TransportState,
    pub time_selection: Option<(f64, f64)>,
    /// Every applied command, for assertions.
    pub applied: Vec<Command>,
    /// Fixed meter peaks (linear) that replace the generated ones.
    pub meter_overrides: HashMap<TrackId, f32>,
    change_token: u64,
}

impl FakeDaw {
    /// Same layout as `Mother Project.RPP` from reaper-remote-bandui.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn mother_project() -> Self {
        const AUDIO: u32 = 0x37_1b_c0;
        const INPUT: u32 = 0x80_00_02;
        const OUTPUT: u32 = 0x00_f4_2b;

        const AUDIO_FOLDER: Option<TrackId> = Some(3);
        const INPUT_FOLDER: Option<TrackId> = Some(6);
        const OUTPUT_FOLDER: Option<TrackId> = Some(11);

        let plain = |id: TrackId, name: &str, color: Option<u32>, volume: f32, parent| FakeTrack {
            id,
            name: name.into(),
            color,
            volume,
            hw_out: false,
            receives: vec![],
            parent,
        };
        let output = |id: TrackId, name: &str, volume: f32, receives: &[(TrackId, f32, bool)]| FakeTrack {
            id,
            name: name.into(),
            color: Some(OUTPUT),
            volume,
            hw_out: true,
            receives: receives
                .iter()
                .map(|&(src, volume, mute)| FakeReceive { src, volume, mute })
                .collect(),
            parent: OUTPUT_FOLDER,
        };

        let region = |id, name: &str, start, end, color| Region {
            id,
            name: name.into(),
            start,
            end,
            color,
        };
        let marker = |id, name: &str, pos, color| Marker {
            id,
            name: name.into(),
            pos,
            color,
        };

        Self {
            tracks: vec![
                plain(1, "MIDI", None, 0.44, None),
                plain(2, "Click", Some(0xfb_02_ff), 1.0, None),
                plain(3, "Audio", Some(AUDIO), 1.0, None),
                plain(4, "Backtrack", Some(AUDIO), 1.0, AUDIO_FOLDER),
                plain(5, "Vocal Backtrack", Some(AUDIO), 1.0, AUDIO_FOLDER),
                plain(6, "Inputs", Some(INPUT), 1.0, None),
                plain(7, "Drum mic [1]", Some(INPUT), 1.0, INPUT_FOLDER),
                plain(8, "Bass [3]", Some(INPUT), 1.0, INPUT_FOLDER),
                plain(9, "Guitar [4]", Some(INPUT), 1.0, INPUT_FOLDER),
                plain(10, "Vocals [5]", Some(INPUT), 1.0, INPUT_FOLDER),
                plain(11, "Outputs", None, 1.0, None),
                output(12, "PA [1+2]", 1.0, &[(4, 1.0, true), (5, 1.0, true)]),
                output(
                    13,
                    "Drums [3+4]",
                    0.42,
                    &[
                        (2, 0.99, false),
                        (4, 0.89, false),
                        (5, 0.77, true),
                        (7, 0.36, false),
                        (8, 0.82, false),
                        (9, 0.72, false),
                        (10, 0.83, false),
                    ],
                ),
                output(
                    14,
                    "Bass [5+6]",
                    1.0,
                    &[
                        (2, 1.0, false),
                        (4, 1.0, false),
                        (5, 1.0, false),
                        (7, 1.0, false),
                        (8, 1.0, false),
                        (9, 1.0, false),
                        (10, 1.0, false),
                    ],
                ),
                output(
                    15,
                    "Guitar [7+8]",
                    0.9,
                    &[
                        (2, 1.0, false),
                        (4, 0.75, true),
                        (5, 0.65, false),
                        (7, 1.0, false),
                        (8, 0.84, false),
                        (9, 1.0, false),
                        (10, 0.55, false),
                    ],
                ),
                output(
                    16,
                    "Vocals [9+10]",
                    1.0,
                    &[
                        (2, 1.0, false),
                        (4, 1.0, false),
                        (5, 1.0, false),
                        (7, 1.0, false),
                        (8, 1.0, false),
                        (9, 1.0, false),
                        (10, 1.0, false),
                    ],
                ),
            ],
            regions: vec![
                region(1, "Summer", 0.0, 250.434_72, None),
                region(2, "Autumn", 333.912_96, 550.956_384, Some(0x8b_87_27)),
                region(3, "Winter", 684.521_568, 1_126.956_24, Some(0x3c_1d_8b)),
                region(4, "Spring", 1_268.869_248, 1_736.347_392, Some(0x3e_24_10)),
            ],
            markers: vec![
                marker(1, "Intro", 0.0, None),
                marker(2, "Verse", 33.391_296, None),
                marker(3, "Chrous", 83.478_24, Some(0xa9_ea_d2)),
                marker(4, "Verse", 367.304_256, Some(0x25_21_00)),
                marker(5, "Chorus", 434.086_848, None),
                marker(6, "Start", 684.521_568, None),
            ],
            region_order: crate::model::parse_region_order("11,14,13,12"),
            transport: TransportState::default(),
            time_selection: None,
            applied: vec![],
            meter_overrides: HashMap::new(),
            change_token: 0,
        }
    }

    /// Switches to another project, like opening a project in REAPER.
    pub fn replace_project(&mut self, project: Self) {
        let token = self.change_token;
        *self = project;
        self.change_token = token + 1;
    }

    /// Marks the structure as changed (what REAPER's project state change count does).
    pub fn touch(&mut self) {
        self.change_token += 1;
    }

    pub fn track_mut(&mut self, id: TrackId) -> Option<&mut FakeTrack> {
        self.tracks.iter_mut().find(|t| t.id == id)
    }

    /// Advances playback. Stops at the end of the time selection unless repeat is on, then it loops.
    pub fn advance(&mut self, dt: Duration) {
        if self.transport.play != PlayState::Playing {
            return;
        }
        self.transport.position += dt.as_secs_f64();
        if let Some((start, end)) = self.time_selection
            && self.transport.position >= end
        {
            if self.transport.repeat {
                self.transport.position = start + (self.transport.position - end) % (end - start).max(f64::EPSILON);
            } else {
                self.transport.position = start;
                self.transport.play = PlayState::Stopped;
                self.transport.recording = false;
            }
        }
    }
}

impl Daw for FakeDaw {
    fn change_token(&mut self) -> u64 {
        self.change_token
    }

    fn structure(&mut self) -> Structure {
        Structure {
            tracks: self
                .tracks
                .iter()
                .zip(1..)
                .map(|(t, number)| TrackInfo {
                    id: t.id,
                    number,
                    name: t.name.clone(),
                    color: t.color,
                    output: t.hw_out && !t.receives.is_empty(),
                    receives: t.receives.iter().map(|r| ReceiveInfo { src: r.src }).collect(),
                    parent: t.parent,
                })
                .collect(),
            regions: self.regions.clone(),
            markers: self.markers.clone(),
            region_order: self.region_order.clone(),
        }
    }

    fn mix(&mut self, structure: &Structure) -> Mix {
        Mix {
            tracks: structure
                .tracks
                .iter()
                .map(|info| {
                    self.tracks
                        .iter()
                        .find(|t| t.id == info.id)
                        .map_or_else(TrackMix::default, |t| TrackMix {
                            volume: t.volume,
                            receives: info
                                .receives
                                .iter()
                                .zip(&t.receives)
                                .map(|(_, r)| ReceiveMix {
                                    volume: r.volume,
                                    mute: r.mute,
                                })
                                .collect(),
                        })
                })
                .collect(),
        }
    }

    fn transport(&mut self) -> TransportState {
        self.transport
    }

    /// Overrides, otherwise a wave between about -16 and -4 dBFS (scaled by the track volume) while playing.
    fn meters(&mut self, tracks: &[TrackId]) -> Vec<f32> {
        let playing = self.transport.play == PlayState::Playing;
        tracks
            .iter()
            .map(|id| {
                if let Some(peak) = self.meter_overrides.get(id) {
                    return *peak;
                }
                let Some(track) = self.tracks.iter().find(|t| t.id == *id) else {
                    return 0.0;
                };
                if !playing {
                    return 0.0;
                }
                let rate = 1.3 + f64::from(*id) * 0.71;
                let wave = 0.5 + 0.5 * (self.transport.position * rate).sin();
                // A level, precision is irrelevant.
                #[allow(clippy::cast_possible_truncation)]
                let wave = wave as f32;
                track.volume * (0.15 + 0.45 * wave)
            })
            .collect()
    }

    fn apply(&mut self, command: &Command) -> Result<(), CommandError> {
        self.applied.push(command.clone());
        let t = &mut self.transport;
        match command {
            Command::Play => {
                t.play = PlayState::Playing;
            }
            Command::Pause => {
                if t.play == PlayState::Playing {
                    t.play = PlayState::Paused;
                } else if t.play == PlayState::Paused {
                    t.play = PlayState::Playing;
                }
            }
            Command::Stop => {
                t.play = PlayState::Stopped;
                t.recording = false;
            }
            Command::Record => {
                t.play = PlayState::Playing;
                t.recording = true;
            }
            Command::ToggleRepeat => t.repeat = !t.repeat,
            Command::Goto { region, marker } => {
                let r = self
                    .regions
                    .iter()
                    .find(|r| r.id == *region)
                    .ok_or(CommandError::UnknownRegion(*region))?;
                let pos = match marker {
                    Some(m) => {
                        self.markers
                            .iter()
                            .find(|x| x.id == *m)
                            .ok_or(CommandError::UnknownMarker(*m))?
                            .pos
                    }
                    None => r.start,
                };
                self.time_selection = Some((r.start, r.end));
                self.transport.position = pos;
            }
            Command::SetTrackVolume { track, volume } => {
                self.track_mut(*track).ok_or(CommandError::UnknownTrack(*track))?.volume = *volume;
            }
            Command::SetReceive {
                track,
                index,
                src,
                volume,
                mute,
            } => {
                let error = CommandError::UnknownReceive {
                    track: *track,
                    index: *index,
                    src: *src,
                };
                let t = self.track_mut(*track).ok_or(CommandError::UnknownTrack(*track))?;
                let r = usize::try_from(*index)
                    .ok()
                    .and_then(|i| t.receives.get_mut(i))
                    .filter(|r| r.src == *src)
                    .ok_or(error)?;
                if let Some(v) = volume {
                    r.volume = *v;
                }
                if let Some(m) = mute {
                    r.mute = *m;
                }
            }
            Command::SetRegionOrder(order) => {
                self.region_order.clone_from(order);
            }
        }
        Ok(())
    }
}
