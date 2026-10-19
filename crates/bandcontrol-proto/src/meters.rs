//! One byte peak levels used by `Meters`: 0 is silence (below -60 dBFS), otherwise half dB steps from -60 dBFS.

/// Quietest level that is not silence.
pub const FLOOR_DB: f32 = -60.0;

/// Encodes a linear peak amplitude (1.0 = 0 dBFS).
#[must_use]
pub fn encode(peak: f32) -> u8 {
    let db = 20.0 * peak.log10();
    if db.is_nan() || db < FLOOR_DB {
        return 0;
    }
    let steps = ((db - FLOOR_DB) * 2.0).round() + 1.0;
    // Clamped into the byte range first.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = steps.clamp(1.0, 255.0) as u8;
    byte
}

/// dBFS of an encoded peak, `None` for silence.
#[must_use]
pub fn decode(byte: u8) -> Option<f32> {
    (byte != 0).then(|| f32::from(byte - 1) / 2.0 + FLOOR_DB)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn levels() {
        assert_eq!(encode(0.0), 0);
        assert_eq!(encode(f32::NAN), 0);
        assert_eq!(encode(0.0001), 0);
        assert_eq!(encode(0.001), 1);
        assert_eq!(encode(1.0), 121);
        assert_eq!(encode(2.0), 133);
        assert_eq!(encode(1e9), 255);
        assert_eq!(decode(0), None);
        assert_eq!(decode(1), Some(-60.0));
        assert_eq!(decode(121), Some(0.0));
        assert_eq!(decode(255), Some(67.0));
    }
}
