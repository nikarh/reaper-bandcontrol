//! Keeps clients in sync with the DAW.
//!
//! * A joining session gets the full `State` and `Transport`.
//! * Gains are diffed every tick and broadcast as `GainDelta`.
//! * Structural changes are broadcast as a full `State` after `structure_debounce` of quiet (at most
//!   `structure_max_wait` after the first change). Gain deltas pause while a structural change is pending.
//! * `Transport` is broadcast every `transport_interval` while playing and immediately on discontinuities.
//! * Sessions with a meter subscription get the highest peak of each subscribed track every `meter_interval`.
//!   Peaks are sampled on every tick; frames where everything is silent are only sent once.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::{Duration, Instant},
};

use bandcontrol_proto::{meters, pb, server::SessionId};

use crate::{
    convert,
    daw::Daw,
    model::{Mix, PlayState, Structure, TrackId, TransportState},
};

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub structure_debounce: Duration,
    pub structure_max_wait: Duration,
    /// Structure is re-read at least this often even if the change token did not change.
    pub structure_poll: Duration,
    pub transport_interval: Duration,
    /// Position difference from the extrapolated position that counts as a seek, in seconds.
    pub seek_threshold: f64,
    pub meter_interval: Duration,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            structure_debounce: Duration::from_secs(1),
            structure_max_wait: Duration::from_secs(3),
            structure_poll: Duration::from_secs(2),
            transport_interval: Duration::from_secs(1),
            seek_threshold: 0.25,
            meter_interval: Duration::from_millis(100),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Joined(SessionId),
    Left(SessionId),
    Command(SessionId, pb::ClientMessage),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Output {
    Broadcast(pb::ServerMessage),
    Unicast(SessionId, pb::ServerMessage),
}

#[derive(Debug)]
struct Published {
    structure: Structure,
    mix: Mix,
}

#[derive(Debug)]
struct Pending {
    first_change: Instant,
    last_change: Instant,
    structure: Structure,
}

#[derive(Debug)]
struct MeterSubscription {
    id: u32,
    tracks: Vec<TrackId>,
    /// The last frame sent was all silence.
    silent: bool,
}

#[derive(Debug, Default)]
pub struct Engine {
    config: EngineConfig,
    sessions: BTreeSet<SessionId>,
    published: Option<Published>,
    pending: Option<Pending>,
    last_token: Option<u64>,
    last_structure_read: Option<Instant>,
    last_transport: Option<(TransportState, Instant)>,
    meters: BTreeMap<SessionId, MeterSubscription>,
    /// Highest peak per track since the last meter frame.
    peaks: HashMap<TrackId, f32>,
    next_meter_frame: Option<Instant>,
}

fn message(msg: pb::server_message::Msg) -> pb::ServerMessage {
    pb::ServerMessage { msg: Some(msg) }
}

impl Engine {
    #[must_use]
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn sessions(&self) -> usize {
        self.sessions.len()
    }

    pub fn tick(&mut self, daw: &mut dyn Daw, now: Instant, inputs: Vec<Input>) -> Vec<Output> {
        let mut out = Vec::new();
        let mut joined = Vec::new();
        let mut force_structure = false;

        for input in inputs {
            match input {
                Input::Joined(s) => {
                    if self.sessions.is_empty() {
                        // The host may stop ticking while nobody is connected: start from a fresh snapshot.
                        self.published = None;
                        self.pending = None;
                        self.last_transport = None;
                        self.meters.clear();
                    }
                    self.sessions.insert(s);
                    joined.push(s);
                }
                Input::Left(s) => {
                    self.sessions.remove(&s);
                    self.meters.remove(&s);
                    joined.retain(|j| *j != s);
                }
                Input::Command(
                    s,
                    pb::ClientMessage {
                        msg: Some(pb::client_message::Msg::SubscribeMeters(sub)),
                    },
                ) => self.subscribe_meters(s, sub),
                Input::Command(s, msg) => {
                    let Some(command) = convert::command(msg) else {
                        tracing::debug!(session = s, "ignoring invalid command");
                        continue;
                    };
                    force_structure |= command.affects_structure();
                    if let Err(e) = daw.apply(&command) {
                        tracing::warn!(session = s, ?command, "command failed: {e}");
                    }
                }
            }
        }

        self.check_structure(daw, now, force_structure, &mut out);
        self.check_gains(daw, &mut out);
        self.check_transport(daw, now, &mut out);
        self.check_meters(daw, now, &mut out);

        if !joined.is_empty() {
            let state = self.state_message(daw);
            let transport = convert::transport(&self.last_transport.map_or_else(|| daw.transport(), |(t, _)| t));
            for s in joined {
                out.push(Output::Unicast(
                    s,
                    message(pb::server_message::Msg::State(state.clone())),
                ));
                out.push(Output::Unicast(
                    s,
                    message(pb::server_message::Msg::Transport(transport)),
                ));
            }
        }

        out
    }

    fn subscribe_meters(&mut self, session: SessionId, sub: pb::SubscribeMeters) {
        if sub.tracks.is_empty() || !self.sessions.contains(&session) {
            self.meters.remove(&session);
        } else {
            self.meters.insert(
                session,
                MeterSubscription {
                    id: sub.subscription,
                    tracks: sub.tracks,
                    silent: false,
                },
            );
        }
    }

    fn check_meters(&mut self, daw: &mut dyn Daw, now: Instant, out: &mut Vec<Output>) {
        if self.meters.is_empty() {
            self.peaks.clear();
            return;
        }
        let tracks: Vec<TrackId> = self
            .meters
            .values()
            .flat_map(|m| m.tracks.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        for (track, peak) in tracks.iter().zip(daw.meters(&tracks)) {
            let max = self.peaks.entry(*track).or_insert(0.0);
            *max = max.max(peak);
        }
        if self.next_meter_frame.is_some_and(|t| now < t) {
            return;
        }
        // Deadlines advance by the interval so frames average the interval even with coarse ticks.
        self.next_meter_frame = Some(match self.next_meter_frame {
            Some(t) if now.duration_since(t) < self.config.meter_interval => t + self.config.meter_interval,
            _ => now + self.config.meter_interval,
        });
        for (session, sub) in &mut self.meters {
            let peaks: Vec<u8> = sub
                .tracks
                .iter()
                .map(|t| meters::encode(self.peaks.get(t).copied().unwrap_or(0.0)))
                .collect();
            let silent = peaks.iter().all(|&p| p == 0);
            if silent && sub.silent {
                continue;
            }
            sub.silent = silent;
            out.push(Output::Unicast(
                *session,
                message(pb::server_message::Msg::Meters(pb::Meters {
                    subscription: sub.id,
                    peaks,
                })),
            ));
        }
        self.peaks.clear();
    }

    fn state_message(&mut self, daw: &mut dyn Daw) -> pb::State {
        let published = self.published.get_or_insert_with(|| {
            let structure = daw.structure().relevant();
            let mix = daw.mix(&structure);
            Published { structure, mix }
        });
        convert::state(&published.structure, &published.mix)
    }

    fn check_structure(&mut self, daw: &mut dyn Daw, now: Instant, force: bool, out: &mut Vec<Output>) {
        let token = daw.change_token();
        let poll_due = self
            .last_structure_read
            .is_none_or(|t| now.duration_since(t) >= self.config.structure_poll);
        if force || poll_due || self.last_token != Some(token) {
            self.last_token = Some(token);
            self.last_structure_read = Some(now);
            self.observe_structure(daw.structure().relevant(), now);
        }

        let Some(pending) = &self.pending else {
            return;
        };
        let quiet = now.duration_since(pending.last_change) >= self.config.structure_debounce;
        let overdue = now.duration_since(pending.first_change) >= self.config.structure_max_wait;
        if !(quiet || overdue) {
            return;
        }
        let Some(Pending { structure, .. }) = self.pending.take() else {
            return;
        };
        let mix = daw.mix(&structure);
        let state = convert::state(&structure, &mix);
        self.published = Some(Published { structure, mix });
        if !self.sessions.is_empty() {
            out.push(Output::Broadcast(message(pb::server_message::Msg::State(state))));
        }
    }

    fn observe_structure(&mut self, structure: Structure, now: Instant) {
        let Some(published) = &self.published else {
            // Nothing published yet: publish lazily when the first client joins.
            return;
        };
        if structure == published.structure {
            self.pending = None;
            return;
        }
        match &mut self.pending {
            Some(p) if p.structure == structure => {}
            Some(p) => {
                p.structure = structure;
                p.last_change = now;
            }
            None => {
                self.pending = Some(Pending {
                    first_change: now,
                    last_change: now,
                    structure,
                });
            }
        }
    }

    fn check_gains(&mut self, daw: &mut dyn Daw, out: &mut Vec<Output>) {
        if self.pending.is_some() {
            return;
        }
        let Some(published) = &mut self.published else {
            return;
        };
        let mix = daw.mix(&published.structure);
        if let Some(delta) = convert::gain_delta(&published.structure, &published.mix, &mix) {
            published.mix = mix;
            if !self.sessions.is_empty() {
                out.push(Output::Broadcast(message(pb::server_message::Msg::Gains(delta))));
            }
        }
    }

    fn check_transport(&mut self, daw: &mut dyn Daw, now: Instant, out: &mut Vec<Output>) {
        let current = daw.transport();
        let send = match &self.last_transport {
            None => true,
            Some((last, at)) => {
                let elapsed = now.duration_since(*at);
                let (expected, threshold) = if last.play == PlayState::Playing {
                    (last.position + elapsed.as_secs_f64(), self.config.seek_threshold)
                } else {
                    (last.position, 0.001)
                };
                last.play != current.play
                    || last.repeat != current.repeat
                    || last.recording != current.recording
                    || (current.position - expected).abs() > threshold
                    || (current.play == PlayState::Playing && elapsed >= self.config.transport_interval)
            }
        };
        if !send {
            return;
        }
        self.last_transport = Some((current, now));
        if !self.sessions.is_empty() {
            out.push(Output::Broadcast(message(pb::server_message::Msg::Transport(
                convert::transport(&current),
            ))));
        }
    }
}

#[cfg(test)]
mod tests;
