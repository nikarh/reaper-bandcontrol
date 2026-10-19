//! The interface between the engine and a DAW. All methods are called from the DAW's main thread.

use crate::model::{Mix, RegionOrder, Structure, TrackId, TransportState};

/// Highest linear gain accepted from clients (+12 dB, REAPER's fader maximum).
pub const MAX_VOLUME: f32 = 3.981_071_7;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Stop,
    Record,
    ToggleRepeat,
    /// Select region `region` (time selection) and move the cursor to `marker` or the region start.
    Goto {
        region: u32,
        marker: Option<u32>,
    },
    SetTrackVolume {
        track: TrackId,
        volume: f32,
    },
    SetReceive {
        track: TrackId,
        index: u32,
        src: TrackId,
        volume: Option<f32>,
        mute: Option<bool>,
    },
    SetRegionOrder(Vec<RegionOrder>),
}

impl Command {
    /// Whether the command can change [`Structure`], forcing an immediate structure check.
    #[must_use]
    pub fn affects_structure(&self) -> bool {
        matches!(self, Self::SetRegionOrder(_))
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CommandError {
    #[error("unknown track {0}")]
    UnknownTrack(TrackId),
    #[error("track {track} has no receive {index} from track {src}")]
    UnknownReceive { track: TrackId, index: u32, src: TrackId },
    #[error("unknown region {0}")]
    UnknownRegion(u32),
    #[error("unknown marker {0}")]
    UnknownMarker(u32),
}

pub trait Daw {
    /// Cheap token that changes whenever the project might have changed structurally.
    fn change_token(&mut self) -> u64;

    /// Reads all tracks (with receives), regions, markers and the region order.
    fn structure(&mut self) -> Structure;

    /// Reads the gains of the tracks and receives in `structure`. Tracks that no longer exist read as zero gain.
    fn mix(&mut self, structure: &Structure) -> Mix;

    fn transport(&mut self) -> TransportState;

    fn apply(&mut self, command: &Command) -> Result<(), CommandError>;
}
