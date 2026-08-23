//! Shared vocabulary for the `SubTrackt` pipeline.
//!
//! Every stage in the pipeline described in the [architecture document] speaks in the types
//! defined here, so that no stage crate needs to depend on another stage crate. The dependency
//! graph is a fan: `core` at the hub, each stage crate depending only on it, and the `subtrackt`
//! facade crate wiring them together.
//!
//! [architecture document]: https://github.com/sovereign-media/Sovereign.SubTrackt/issues/1

pub mod bitmap;
pub mod error;
pub mod glyph;
pub mod progress;
pub mod stage;
pub mod subtitle;
pub mod time;

pub use bitmap::{IndexedBitmap, Palette, PaletteEntry, Rect, Rgba8};
pub use error::{Error, Result};
pub use glyph::{
    FEATURE_BITS, FEATURE_GRID, FEATURE_WORDS, FeatureVector, Glyph, GlyphMatch, InkAspect,
    LineMetrics, MarkSlope, SPAN_TENTHS, Slant, UprightSpan,
};
pub use progress::{Phase, Progress, Silent};
pub use stage::{BitmapDecoder, GlyphMatcher, Segmenter, TextAssembler, TrackWriter};
pub use subtitle::{Confidence, Cue, SubtitleFormat, SubtitleImage, TextTrack};
pub use time::{PTS_HZ, TimeSpan, Timestamp};
