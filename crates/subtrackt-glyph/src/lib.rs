//! From subtitle bitmap to identified characters.
//!
//! This crate is the part of the design that is *not* a general OCR engine, and the difference is
//! worth stating plainly: every glyph is normalised to a fixed bit vector and compared to a
//! reference set by Hamming distance, so a character the reference set does not contain comes back
//! as "no match" rather than as a confident wrong answer. The pipeline can then act on that fact.
//!
//! Stages, in order:
//!
//! 1. [`binarize`] — palette alpha to a foreground mask.
//! 2. [`ccl`] — connected components to bounding boxes.
//! 3. [`group`] — merge diacritics onto their base glyph, assign glyphs to lines.
//! 4. [`feature`] — normalise each glyph to a [`subtrackt_core::FeatureVector`];
//!    [`mark`] — read the direction of its diacritic, which that normalisation would otherwise
//!    average away; and [`slant`] — read how far the whole line leans, which its bounding boxes
//!    get wrong in a way the spacing rule two stages later cannot recover from.
//! 5. [`cluster`] — group the stream's own shapes, so a consensus vector is matched
//!    rather than one instance's accidents.
//! 6. [`matcher`] — nearest reference vector, with [`cache`] short-circuiting repeats.

pub mod binarize;
pub mod cache;
pub mod ccl;
pub mod cluster;
pub mod feature;
#[cfg(feature = "font")]
pub mod font;
pub mod group;
pub mod mark;
pub mod matcher;
pub mod metrics;
pub mod reference;
pub mod slant;
pub mod split;

pub use binarize::{Binarizer, BinaryMask, Threshold};
pub use cache::SessionCache;
pub use cluster::{Cluster, ClusterRules, Shapes};
pub use matcher::HammingMatcher;
pub use metrics::{LineAnchors, MetricRules};
pub use reference::{ReferenceEntry, ReferenceSet};
