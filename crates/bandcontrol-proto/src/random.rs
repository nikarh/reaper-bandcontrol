//! Injectable randomness, so session state machines are deterministic in tests.

pub trait RandomSource: Send {
    fn fill(&mut self, buf: &mut [u8]);
}

/// Operating system CSPRNG.
#[derive(Debug, Default, Clone, Copy)]
pub struct OsRandom;

impl RandomSource for OsRandom {
    fn fill(&mut self, buf: &mut [u8]) {
        if let Err(e) = getrandom::fill(buf) {
            // There is no meaningful way to continue without randomness: nonces and keys would be predictable.
            #[allow(clippy::panic)]
            {
                panic!("OS random number generator failed: {e}");
            }
        }
    }
}

/// Deterministic byte counter for tests and test vectors. Never use for anything real.
#[derive(Debug, Default, Clone)]
pub struct CountingRandom {
    next: u8,
}

impl CountingRandom {
    #[must_use]
    pub fn starting_at(next: u8) -> Self {
        Self { next }
    }
}

impl RandomSource for CountingRandom {
    fn fill(&mut self, buf: &mut [u8]) {
        for b in buf {
            *b = self.next;
            self.next = self.next.wrapping_add(1);
        }
    }
}

/// Fills a fixed size array from a dynamically dispatched source.
pub fn array<const N: usize>(rng: &mut dyn RandomSource) -> [u8; N] {
    let mut out = [0; N];
    rng.fill(&mut out);
    out
}
