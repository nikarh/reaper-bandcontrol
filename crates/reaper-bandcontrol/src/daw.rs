//! [`Daw`] implementation backed by the REAPER API.

use std::collections::HashMap;

use bandcontrol_core::{
    daw::{Command, CommandError, Daw},
    model::{
        Marker, Mix, PlayState, ReceiveInfo, ReceiveMix, Region, Structure, TrackId, TrackInfo, TrackMix,
        TransportState, format_region_order, parse_region_order,
    },
};

use crate::ffi::{Reaper, Track};

const RECEIVES: i32 = -1;
const HW_OUTPUTS: i32 = 1;
const CUSTOM_COLOR: i32 = 0x0100_0000;
const MAX_METER_CHANNELS: i32 = 64;

const EXT_SECTION: &std::ffi::CStr = c"BANDUI";
const EXT_REGIONS: &std::ffi::CStr = c"regions";

// Native REAPER action ids, same as the web remote uses.
const ACTION_PLAY: i32 = 1007;
const ACTION_PAUSE: i32 = 1008;
const ACTION_STOP: i32 = 40667;
const ACTION_RECORD: i32 = 1013;
const ACTION_TOGGLE_REPEAT: i32 = 1068;

/// Custom track/marker color as `0xRRGGBB`. The flag with black reads as no color, it shows as the theme default.
fn color(native: i32) -> Option<u32> {
    if native & CUSTOM_COLOR == 0 || native == CUSTOM_COLOR {
        return None;
    }
    let native = u32::try_from(native & 0x00ff_ffff).unwrap_or(0);
    Some(if cfg!(target_os = "windows") {
        // COLORREF is 0x00BBGGRR.
        ((native & 0xff) << 16) | (native & 0xff00) | (native >> 16)
    } else {
        // SWELL's RGB() is 0x00RRGGBB.
        native
    })
}

#[allow(clippy::cast_possible_truncation)]
fn to_f32(v: f64) -> f32 {
    v as f32
}

#[allow(clippy::cast_possible_truncation)]
fn to_i32(v: f64) -> i32 {
    v as i32
}

/// Tracks are identified by small ids assigned per track GUID, stable while REAPER runs.
#[derive(Debug)]
pub struct ReaperDaw {
    reaper: &'static Reaper,
    ids: HashMap<[u8; 16], TrackId>,
    next_id: TrackId,
}

impl ReaperDaw {
    #[must_use]
    pub fn new(reaper: &'static Reaper) -> Self {
        Self {
            reaper,
            ids: HashMap::new(),
            next_id: 1,
        }
    }

    fn id_of(&mut self, track: Track) -> TrackId {
        let guid = self.reaper.track_guid(track);
        *self.ids.entry(guid).or_insert_with(|| {
            let id = self.next_id;
            self.next_id += 1;
            id
        })
    }

    fn scan(&mut self) -> Vec<(TrackId, Track)> {
        let tracks: Vec<Track> = self.reaper.tracks().collect();
        tracks.into_iter().map(|t| (self.id_of(t), t)).collect()
    }

    fn find(&mut self, id: TrackId) -> Option<Track> {
        self.scan().into_iter().find(|(i, _)| *i == id).map(|(_, t)| t)
    }
}

impl Daw for ReaperDaw {
    fn change_token(&mut self) -> u64 {
        self.reaper.change_token()
    }

    fn structure(&mut self) -> Structure {
        let r = self.reaper;
        let tracks = self.scan();
        let track_infos = tracks
            .iter()
            .zip(1..)
            .map(|(&(id, track), number)| {
                let receive_count = r.send_count(track, RECEIVES);
                let receives = (0..receive_count)
                    .map(|i| ReceiveInfo {
                        src: r.receive_source(track, i).map_or(0, |src| self.id_of(src)),
                    })
                    .collect();
                TrackInfo {
                    id,
                    number,
                    name: r.track_name(track),
                    color: color(to_i32(r.track_info(track, c"I_CUSTOMCOLOR"))),
                    output: r.send_count(track, HW_OUTPUTS) > 0 && receive_count > 0,
                    receives,
                    parent: r.parent_track(track).map(|p| self.id_of(p)),
                }
            })
            .collect();

        let mut regions = vec![];
        let mut markers = vec![];
        for m in r.markers() {
            let id = u32::try_from(m.number).unwrap_or(0);
            if m.is_region {
                regions.push(Region {
                    id,
                    name: m.name,
                    start: m.pos,
                    end: m.end,
                    color: color(m.color),
                });
            } else {
                markers.push(Marker {
                    id,
                    name: m.name,
                    pos: m.pos,
                    color: color(m.color),
                });
            }
        }

        Structure {
            tracks: track_infos,
            regions,
            markers,
            region_order: parse_region_order(&r.proj_ext_state(EXT_SECTION, EXT_REGIONS)),
        }
    }

    fn meters(&mut self, tracks: &[TrackId]) -> Vec<f32> {
        let r = self.reaper;
        let by_id: HashMap<TrackId, Track> = self.scan().into_iter().collect();
        tracks
            .iter()
            .map(|id| {
                by_id.get(id).map_or(0.0, |&track| {
                    let channels = to_i32(r.track_info(track, c"I_NCHAN")).clamp(1, MAX_METER_CHANNELS);
                    to_f32((0..channels).map(|c| r.track_peak(track, c)).fold(0.0, f64::max))
                })
            })
            .collect()
    }

    fn mix(&mut self, structure: &Structure) -> Mix {
        let r = self.reaper;
        let by_id: HashMap<TrackId, Track> = self.scan().into_iter().collect();
        Mix {
            tracks: structure
                .tracks
                .iter()
                .map(|info| {
                    let Some(&track) = by_id.get(&info.id) else {
                        return TrackMix::default();
                    };
                    TrackMix {
                        volume: to_f32(r.track_info(track, c"D_VOL")),
                        receives: (0..info.receives.len())
                            .map(|i| {
                                let i = i32::try_from(i).unwrap_or(i32::MAX);
                                ReceiveMix {
                                    volume: to_f32(r.send_info(track, RECEIVES, i, c"D_VOL")),
                                    mute: r.send_info(track, RECEIVES, i, c"B_MUTE") != 0.0,
                                }
                            })
                            .collect(),
                    }
                })
                .collect(),
        }
    }

    fn transport(&mut self) -> TransportState {
        let r = self.reaper;
        let state = r.play_state();
        let play = if state & 1 != 0 {
            PlayState::Playing
        } else if state & 2 != 0 {
            PlayState::Paused
        } else {
            PlayState::Stopped
        };
        TransportState {
            play,
            position: if state & 3 != 0 {
                r.play_position()
            } else {
                r.cursor_position()
            },
            repeat: r.repeat(),
            recording: state & 4 != 0,
        }
    }

    fn apply(&mut self, command: &Command) -> Result<(), CommandError> {
        let r = self.reaper;
        match command {
            Command::Play => r.command(ACTION_PLAY),
            Command::Pause => r.command(ACTION_PAUSE),
            Command::Stop => r.command(ACTION_STOP),
            Command::Record => r.command(ACTION_RECORD),
            Command::ToggleRepeat => r.command(ACTION_TOGGLE_REPEAT),
            Command::Goto { region, marker } => {
                let all = r.markers();
                let number = |n: u32| i32::try_from(n).unwrap_or(-1);
                let reg = all
                    .iter()
                    .find(|m| m.is_region && m.number == number(*region))
                    .ok_or(CommandError::UnknownRegion(*region))?;
                let pos = match marker {
                    Some(id) => {
                        all.iter()
                            .find(|m| !m.is_region && m.number == number(*id))
                            .ok_or(CommandError::UnknownMarker(*id))?
                            .pos
                    }
                    None => reg.pos,
                };
                r.set_time_selection(reg.pos, reg.end);
                r.set_cursor(pos);
            }
            Command::SetTrackVolume { track, volume } => {
                let t = self.find(*track).ok_or(CommandError::UnknownTrack(*track))?;
                r.set_track_info(t, c"D_VOL", f64::from(*volume));
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
                let t = self.find(*track).ok_or(CommandError::UnknownTrack(*track))?;
                let i = i32::try_from(*index).map_err(|_| error.clone())?;
                let actual_src = r.receive_source(t, i).map(|s| self.id_of(s));
                if actual_src != Some(*src) {
                    return Err(error);
                }
                if let Some(v) = volume {
                    r.set_send_info(t, RECEIVES, i, c"D_VOL", f64::from(*v));
                }
                if let Some(m) = mute {
                    r.set_send_info(t, RECEIVES, i, c"B_MUTE", if *m { 1.0 } else { 0.0 });
                }
            }
            Command::SetRegionOrder(order) => {
                r.set_proj_ext_state(EXT_SECTION, EXT_REGIONS, &format_region_order(order));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn colors_without_custom_flag_are_default() {
        assert_eq!(color(0), None);
        assert_eq!(color(0x8b_8727), None);
        assert_eq!(color(0x0100_0000), None);
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn swell_colors_are_rgb() {
        assert_eq!(color(25_921_319), Some(0x8b_8727));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn windows_colors_are_bgr() {
        assert_eq!(color(0x0127_878b), Some(0x8b_8727));
    }
}
