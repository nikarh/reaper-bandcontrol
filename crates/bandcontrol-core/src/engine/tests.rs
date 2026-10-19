#![allow(clippy::float_cmp)]

use pretty_assertions::assert_eq;

use super::*;
use crate::fake::FakeDaw;

const TICK: Duration = Duration::from_millis(33);

struct Harness {
    engine: Engine,
    daw: FakeDaw,
    now: Instant,
}

impl Harness {
    fn new() -> Self {
        Self {
            engine: Engine::new(EngineConfig::default()),
            daw: FakeDaw::mother_project(),
            now: Instant::now(),
        }
    }

    fn joined() -> Self {
        let mut h = Self::new();
        let out = h.tick(vec![Input::Joined(1)]);
        assert_eq!(out.len(), 3, "{out:?}"); // transport broadcast + state + transport unicast
        h
    }

    fn tick(&mut self, inputs: Vec<Input>) -> Vec<Output> {
        self.engine.tick(&mut self.daw, self.now, inputs)
    }

    /// Advances time in ticks, returns everything emitted.
    fn run(&mut self, duration: Duration) -> Vec<Output> {
        let mut out = vec![];
        let mut elapsed = Duration::ZERO;
        while elapsed < duration {
            self.now += TICK;
            self.daw.advance(TICK);
            elapsed += TICK;
            out.extend(self.tick(vec![]));
        }
        out
    }
}

fn msgs(out: &[Output]) -> Vec<&pb::server_message::Msg> {
    out.iter()
        .filter_map(|o| match o {
            Output::Broadcast(m) | Output::Unicast(_, m) => m.msg.as_ref(),
        })
        .collect()
}

fn states(out: &[Output]) -> Vec<pb::State> {
    msgs(out)
        .into_iter()
        .filter_map(|m| match m {
            pb::server_message::Msg::State(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

fn transports(out: &[Output]) -> Vec<pb::Transport> {
    msgs(out)
        .into_iter()
        .filter_map(|m| match m {
            pb::server_message::Msg::Transport(t) => Some(*t),
            _ => None,
        })
        .collect()
}

fn gains(out: &[Output]) -> Vec<pb::GainDelta> {
    msgs(out)
        .into_iter()
        .filter_map(|m| match m {
            pb::server_message::Msg::Gains(g) => Some(g.clone()),
            _ => None,
        })
        .collect()
}

fn command(msg: pb::client_message::Msg) -> Input {
    Input::Command(1, pb::ClientMessage { msg: Some(msg) })
}

#[test]
fn joining_session_gets_state_and_transport() {
    let mut h = Harness::new();
    let out = h.tick(vec![Input::Joined(7)]);
    let [Output::Unicast(7, state), Output::Unicast(7, transport)] = &out[1..] else {
        panic!("unexpected output {out:?}");
    };
    let Some(pb::server_message::Msg::State(state)) = &state.msg else {
        panic!("expected state");
    };
    let ids: Vec<u32> = state.tracks.iter().map(|t| t.id).collect();
    assert_eq!(ids, (1..=16).collect::<Vec<_>>());
    assert_eq!(state.tracks[6].parent, Some(6));
    assert_eq!(state.tracks[1].receives, vec![]);
    let drums = &state.tracks[12];
    assert_eq!(drums.name, "Drums [3+4]");
    assert!(drums.output);
    assert_eq!(drums.receives.len(), 7);
    assert_eq!(
        drums.receives[2],
        pb::Receive {
            src: 5,
            volume: 0.77,
            mute: true
        }
    );
    assert_eq!(state.regions.len(), 4);
    assert_eq!(state.markers.len(), 6);
    assert_eq!(state.region_order[1], pb::RegionOrder { id: 4, disabled: false });
    assert!(matches!(transport.msg, Some(pb::server_message::Msg::Transport(_))));
}

#[test]
fn nothing_is_broadcast_without_sessions() {
    let mut h = Harness::new();
    h.daw.transport.play = PlayState::Playing;
    h.daw.track_mut(13).unwrap().volume = 0.1;
    assert_eq!(h.run(Duration::from_secs(3)), vec![]);
}

#[test]
fn gain_changes_are_sent_as_deltas_once() {
    let mut h = Harness::joined();
    h.daw.track_mut(13).unwrap().volume = 0.5;
    h.daw.track_mut(15).unwrap().receives[3].mute = true;
    let out = h.run(TICK);
    assert_eq!(
        gains(&out),
        vec![pb::GainDelta {
            tracks: vec![pb::TrackGain { track: 13, volume: 0.5 }],
            receives: vec![pb::ReceiveGain {
                track: 15,
                index: 3,
                volume: 1.0,
                mute: true
            }],
        }]
    );
    assert_eq!(h.run(Duration::from_secs(1)), vec![]);
}

#[test]
fn client_commands_are_applied_and_echoed_as_deltas() {
    let mut h = Harness::joined();
    let out = h.tick(vec![command(pb::client_message::Msg::SetReceive(pb::SetReceive {
        track: 14,
        index: 0,
        src: 2,
        volume: Some(0.25),
        mute: None,
    }))]);
    assert_eq!(gains(&out)[0].receives[0].volume, 0.25);

    // A receive with a mismatching source is not touched.
    let out = h.tick(vec![command(pb::client_message::Msg::SetReceive(pb::SetReceive {
        track: 14,
        index: 0,
        src: 3,
        volume: Some(0.75),
        mute: None,
    }))]);
    assert_eq!(gains(&out), vec![]);
}

#[test]
fn structure_changes_are_debounced() {
    let mut h = Harness::joined();
    h.daw.track_mut(2).unwrap().name = "Metronome".into();
    h.daw.touch();
    // While pending, gain changes are held back.
    h.daw.track_mut(13).unwrap().volume = 0.3;

    let out = h.run(Duration::from_millis(900));
    assert_eq!(states(&out), vec![]);
    assert_eq!(gains(&out), vec![]);

    let out = h.run(Duration::from_millis(200));
    let s = states(&out);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].tracks[1].name, "Metronome");
    assert_eq!(s[0].tracks[12].volume, 0.3);
    assert_eq!(h.run(Duration::from_secs(3)), vec![]);
}

#[test]
fn continuous_structure_changes_are_flushed_after_max_wait() {
    let mut h = Harness::joined();
    let mut out = vec![];
    for i in 0..10 {
        h.daw.track_mut(2).unwrap().name = format!("Click {i}");
        h.daw.touch();
        out.extend(h.run(Duration::from_millis(500)));
    }
    // 5 s of changes every 0.5 s: flushed at ~3 s, then again 3 s later is beyond the window.
    assert_eq!(states(&out).len(), 1);
    let out = h.run(Duration::from_secs(2));
    assert_eq!(states(&out)[0].tracks[1].name, "Click 9");
}

#[test]
fn reverted_structure_change_is_not_sent() {
    let mut h = Harness::joined();
    h.daw.track_mut(2).unwrap().name = "Metronome".into();
    h.daw.touch();
    h.run(Duration::from_millis(300));
    h.daw.track_mut(2).unwrap().name = "Click".into();
    h.daw.touch();
    assert_eq!(h.run(Duration::from_secs(3)), vec![]);
}

#[test]
fn structure_changes_without_token_change_are_found_by_polling() {
    let mut h = Harness::joined();
    h.daw.markers.pop();
    let out = h.run(Duration::from_millis(3100));
    assert_eq!(states(&out).len(), 1);
}

#[test]
fn region_order_command_triggers_state() {
    let mut h = Harness::joined();
    let out = h.tick(vec![command(pb::client_message::Msg::SetRegionOrder(
        pb::SetRegionOrder {
            order: vec![pb::RegionOrder { id: 2, disabled: true }],
        },
    ))]);
    assert_eq!(out, vec![]);
    let out = h.run(Duration::from_millis(1100));
    assert_eq!(
        states(&out)[0].region_order,
        vec![pb::RegionOrder { id: 2, disabled: true }]
    );
}

#[test]
fn transport_is_sent_periodically_while_playing_and_on_changes() {
    let mut h = Harness::joined();
    assert_eq!(h.run(Duration::from_secs(3)), vec![]);

    let out = h.tick(vec![command(pb::client_message::Msg::Transport(
        pb::TransportAction::Play.into(),
    ))]);
    assert_eq!(transports(&out)[0].state, i32::from(pb::PlayState::Playing));

    let out = h.run(Duration::from_millis(3100));
    let t = transports(&out);
    assert_eq!(t.len(), 3, "{t:?}");
    assert!(t[2].pos_ms >= 3000 && t[2].pos_ms <= 3100, "{t:?}");

    // Seek while playing is sent immediately.
    h.daw.transport.position = 100.0;
    let out = h.run(TICK);
    assert_eq!(transports(&out).len(), 1);

    let out = h.tick(vec![command(pb::client_message::Msg::Transport(
        pb::TransportAction::Stop.into(),
    ))]);
    assert_eq!(transports(&out)[0].state, i32::from(pb::PlayState::Stopped));
    assert_eq!(h.run(Duration::from_secs(2)), vec![]);

    // Cursor moved while stopped.
    let out = h.tick(vec![command(pb::client_message::Msg::Goto(pb::Goto {
        region: 2,
        marker: Some(4),
    }))]);
    assert_eq!(transports(&out)[0].pos_ms, 367_304);
    assert_eq!(h.daw.time_selection, Some((333.912_96, 550.956_384)));
}

#[test]
fn left_session_stops_receiving_and_rejoin_gets_state() {
    let mut h = Harness::joined();
    h.tick(vec![Input::Left(1)]);
    assert_eq!(h.engine.sessions(), 0);
    h.daw.track_mut(13).unwrap().volume = 0.2;
    assert_eq!(h.run(TICK), vec![]);
    let out = h.tick(vec![Input::Joined(2)]);
    assert_eq!(states(&out)[0].tracks[12].volume, 0.2);
}

fn subscribe(tracks: &[u32]) -> Input {
    command(pb::client_message::Msg::SubscribeMeters(pb::SubscribeMeters {
        subscription: 5,
        tracks: tracks.to_vec(),
    }))
}

fn meters(out: &[Output]) -> Vec<(SessionId, pb::Meters)> {
    out.iter()
        .filter_map(|o| match o {
            Output::Unicast(
                s,
                pb::ServerMessage {
                    msg: Some(pb::server_message::Msg::Meters(m)),
                },
            ) => Some((*s, m.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn meters_are_sent_to_subscribers_at_the_meter_interval() {
    let mut h = Harness::joined();
    h.daw.meter_overrides.insert(7, 1.0);
    let out = h.tick(vec![subscribe(&[7, 8, 99])]);
    assert_eq!(
        meters(&out),
        vec![(
            1,
            pb::Meters {
                subscription: 5,
                peaks: vec![121, 0, 0]
            }
        )]
    );

    // About one frame per 100 ms with the highest peak in between.
    h.daw.meter_overrides.insert(8, 0.5);
    let out = h.run(Duration::from_millis(1000));
    let frames = meters(&out);
    assert!((9..=11).contains(&frames.len()), "{frames:?}");
    assert_eq!(frames[0].1.peaks, vec![121, 109, 0]);

    let out = h.tick(vec![subscribe(&[])]);
    assert_eq!(meters(&out), vec![]);
    assert_eq!(meters(&h.run(Duration::from_secs(1))), vec![]);
}

#[test]
fn peaks_between_frames_are_kept() {
    let mut h = Harness::joined();
    h.tick(vec![subscribe(&[7])]);
    h.run(Duration::from_millis(50));
    h.daw.meter_overrides.insert(7, 1.0);
    h.run(TICK);
    h.daw.meter_overrides.remove(&7);
    let frames = meters(&h.run(Duration::from_millis(100)));
    assert_eq!(frames[0].1.peaks, vec![121]);
}

#[test]
fn silence_is_sent_once() {
    let mut h = Harness::joined();
    let out = h.tick(vec![subscribe(&[7])]);
    assert_eq!(meters(&out)[0].1.peaks, vec![0]);
    assert_eq!(meters(&h.run(Duration::from_secs(1))), vec![]);

    h.daw.transport.play = PlayState::Playing;
    let frames = meters(&h.run(Duration::from_millis(500)));
    assert!(frames.iter().all(|(_, m)| m.peaks[0] > 0), "{frames:?}");
}

#[test]
fn meter_subscriptions_end_with_the_session() {
    let mut h = Harness::joined();
    h.daw.meter_overrides.insert(7, 1.0);
    h.tick(vec![subscribe(&[7])]);
    h.tick(vec![Input::Joined(2)]);
    h.tick(vec![Input::Left(1)]);
    assert_eq!(meters(&h.run(Duration::from_secs(1))), vec![]);
}
