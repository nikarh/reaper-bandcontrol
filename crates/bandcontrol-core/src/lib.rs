//! DAW abstraction and the engine that keeps phones in sync with it.
//!
//! The [`Engine`](engine::Engine) is sans-IO: it is ticked with a [`Daw`](daw::Daw) implementation, the current
//! time and the inputs from the session layer, and returns the messages to send. REAPER and the in-memory
//! [`FakeDaw`](fake::FakeDaw) implement [`Daw`](daw::Daw).

pub mod convert;
pub mod daw;
pub mod engine;
pub mod fake;
pub mod model;
pub mod rpp;
