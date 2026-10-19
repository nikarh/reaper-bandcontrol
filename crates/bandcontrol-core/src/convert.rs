//! Conversions between the domain model and protobuf messages.

use bandcontrol_proto::pb;

use crate::{
    daw::{Command, MAX_VOLUME},
    model::{Mix, PlayState, RegionOrder, Structure, TransportState},
};

#[must_use]
pub fn seconds_to_ms(seconds: f64) -> u32 {
    let ms = (seconds * 1000.0).round();
    if ms.is_nan() || ms <= 0.0 {
        0
    } else if ms >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        // In range and non-negative, checked above.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ms = ms as u32;
        ms
    }
}

#[must_use]
pub fn state(structure: &Structure, mix: &Mix) -> pb::State {
    pb::State {
        tracks: structure
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let m = mix.tracks.get(i);
                pb::Track {
                    id: t.id,
                    number: t.number,
                    name: t.name.clone(),
                    color: t.color,
                    volume: m.map_or(0.0, |m| m.volume),
                    output: t.output,
                    receives: t
                        .receives
                        .iter()
                        .enumerate()
                        .map(|(j, r)| {
                            let rm = m.and_then(|m| m.receives.get(j)).copied().unwrap_or_default();
                            pb::Receive {
                                src: r.src,
                                volume: rm.volume,
                                mute: rm.mute,
                            }
                        })
                        .collect(),
                    parent: t.parent,
                }
            })
            .collect(),
        regions: structure
            .regions
            .iter()
            .map(|r| pb::Region {
                id: r.id,
                name: r.name.clone(),
                start_ms: seconds_to_ms(r.start),
                end_ms: seconds_to_ms(r.end),
                color: r.color,
            })
            .collect(),
        markers: structure
            .markers
            .iter()
            .map(|m| pb::Marker {
                id: m.id,
                name: m.name.clone(),
                pos_ms: seconds_to_ms(m.pos),
                color: m.color,
            })
            .collect(),
        region_order: structure
            .region_order
            .iter()
            .map(|o| pb::RegionOrder {
                id: o.id,
                disabled: o.disabled,
            })
            .collect(),
    }
}

#[must_use]
pub fn transport(t: &TransportState) -> pb::Transport {
    pb::Transport {
        state: match t.play {
            PlayState::Stopped => pb::PlayState::Stopped,
            PlayState::Playing => pb::PlayState::Playing,
            PlayState::Paused => pb::PlayState::Paused,
        }
        .into(),
        pos_ms: seconds_to_ms(t.position),
        repeat: t.repeat,
        recording: t.recording,
    }
}

/// Gain deltas between two mixes of the same structure. `None` when nothing changed.
#[must_use]
pub fn gain_delta(structure: &Structure, old: &Mix, new: &Mix) -> Option<pb::GainDelta> {
    let mut delta = pb::GainDelta::default();
    for (i, track) in structure.tracks.iter().enumerate() {
        let (Some(o), Some(n)) = (old.tracks.get(i), new.tracks.get(i)) else {
            continue;
        };
        if o.volume.to_bits() != n.volume.to_bits() {
            delta.tracks.push(pb::TrackGain {
                track: track.id,
                volume: n.volume,
            });
        }
        for (j, (ro, rn)) in o.receives.iter().zip(&n.receives).enumerate() {
            if ro.volume.to_bits() != rn.volume.to_bits() || ro.mute != rn.mute {
                delta.receives.push(pb::ReceiveGain {
                    track: track.id,
                    index: u32::try_from(j).unwrap_or(u32::MAX),
                    volume: rn.volume,
                    mute: rn.mute,
                });
            }
        }
    }
    (!delta.tracks.is_empty() || !delta.receives.is_empty()).then_some(delta)
}

fn volume(v: f32) -> Option<f32> {
    v.is_finite().then(|| v.clamp(0.0, MAX_VOLUME))
}

/// Validates a client message. `None` for unknown or invalid commands.
#[must_use]
pub fn command(msg: pb::ClientMessage) -> Option<Command> {
    use pb::client_message::Msg;

    Some(match msg.msg? {
        Msg::Transport(action) => match pb::TransportAction::try_from(action).ok()? {
            pb::TransportAction::Unspecified => return None,
            pb::TransportAction::Play => Command::Play,
            pb::TransportAction::Pause => Command::Pause,
            pb::TransportAction::Stop => Command::Stop,
            pb::TransportAction::Record => Command::Record,
            pb::TransportAction::ToggleRepeat => Command::ToggleRepeat,
        },
        Msg::Goto(g) => Command::Goto {
            region: g.region,
            marker: g.marker,
        },
        Msg::SetTrackVolume(s) => Command::SetTrackVolume {
            track: s.track,
            volume: volume(s.volume)?,
        },
        Msg::SetReceive(s) => {
            let v = match s.volume {
                Some(v) => Some(volume(v)?),
                None => None,
            };
            Command::SetReceive {
                track: s.track,
                index: s.index,
                src: s.src,
                volume: v,
                mute: s.mute,
            }
        }
        Msg::SetRegionOrder(o) => Command::SetRegionOrder(
            o.order
                .into_iter()
                .map(|o| RegionOrder {
                    id: o.id,
                    disabled: o.disabled,
                })
                .collect(),
        ),
        // Session state, handled by the engine.
        Msg::SubscribeMeters(_) => return None,
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn ms_conversion_clamps() {
        assert_eq!(seconds_to_ms(-1.0), 0);
        assert_eq!(seconds_to_ms(f64::NAN), 0);
        assert_eq!(seconds_to_ms(1.2345), 1235);
        assert_eq!(seconds_to_ms(1e12), u32::MAX);
    }

    #[test]
    fn invalid_volumes_are_rejected_or_clamped() {
        let set = |volume| pb::ClientMessage {
            msg: Some(pb::client_message::Msg::SetTrackVolume(pb::SetTrackVolume {
                track: 1,
                volume,
            })),
        };
        assert_eq!(command(set(f32::NAN)), None);
        assert_eq!(
            command(set(100.0)),
            Some(Command::SetTrackVolume {
                track: 1,
                volume: MAX_VOLUME
            })
        );
        assert_eq!(command(pb::ClientMessage { msg: None }), None);
        assert_eq!(
            command(pb::ClientMessage {
                msg: Some(pb::client_message::Msg::Transport(99))
            }),
            None
        );
    }
}
