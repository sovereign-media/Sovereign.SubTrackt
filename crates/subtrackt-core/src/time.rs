//! Presentation timestamps.
//!
//! Both PGS and VOBSUB carry timing as 90 kHz ticks, so that is the native unit here. Conversion
//! to wall-clock only happens at the output-formatting boundary.

use std::fmt;

/// Ticks per second in an MPEG presentation timestamp.
pub const PTS_HZ: u64 = 90_000;

/// A presentation timestamp, in 90 kHz ticks since the start of the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Timestamp(u64);

impl Timestamp {
    /// The zero timestamp.
    pub const ZERO: Self = Self(0);

    /// Build a timestamp from raw 90 kHz ticks.
    #[must_use]
    pub const fn from_ticks(ticks: u64) -> Self {
        Self(ticks)
    }

    /// Build a timestamp from whole milliseconds.
    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(millis * PTS_HZ / 1_000)
    }

    /// The raw 90 kHz tick count.
    #[must_use]
    pub const fn ticks(self) -> u64 {
        self.0
    }

    /// The timestamp rounded down to whole milliseconds.
    #[must_use]
    pub const fn as_millis(self) -> u64 {
        self.0 * 1_000 / PTS_HZ
    }

    /// Split into `(hours, minutes, seconds, milliseconds)` for subtitle formatting.
    #[must_use]
    pub const fn hmsm(self) -> (u64, u64, u64, u64) {
        let ms = self.as_millis();
        (ms / 3_600_000, (ms / 60_000) % 60, (ms / 1_000) % 60, ms % 1_000)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (h, m, s, ms) = self.hmsm();
        write!(f, "{h:02}:{m:02}:{s:02}.{ms:03}")
    }
}

/// The interval a subtitle is on screen: `start` inclusive, `end` exclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeSpan {
    /// When the subtitle appears.
    pub start: Timestamp,
    /// When the subtitle is cleared.
    pub end: Timestamp,
}

impl TimeSpan {
    /// Construct a span, clamping `end` up to `start` if the source timing was inverted.
    #[must_use]
    pub fn new(start: Timestamp, end: Timestamp) -> Self {
        Self { start, end: end.max(start) }
    }

    /// Duration of the span in 90 kHz ticks.
    #[must_use]
    pub const fn ticks(self) -> u64 {
        self.end.ticks() - self.start.ticks()
    }

    /// Whether the span covers no time at all, which usually means a dropped cue.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.ticks() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millis_round_trip_through_ticks() {
        assert_eq!(Timestamp::from_millis(1_234).as_millis(), 1_234);
        assert_eq!(Timestamp::from_millis(1_000).ticks(), PTS_HZ);
    }

    #[test]
    fn hmsm_splits_a_long_timestamp() {
        // 1h 02m 03.004s
        let t = Timestamp::from_millis(3_723_004);
        assert_eq!(t.hmsm(), (1, 2, 3, 4));
        assert_eq!(t.to_string(), "01:02:03.004");
    }

    #[test]
    fn inverted_spans_are_clamped_rather_than_negative() {
        let span = TimeSpan::new(Timestamp::from_millis(500), Timestamp::from_millis(100));
        assert!(span.is_empty());
        assert_eq!(span.end, span.start);
    }
}
