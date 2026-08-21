//! Reassembling matched glyphs into text, and writing that text out.

pub mod correct;
pub mod format;
pub mod layout;

pub use correct::{NoopCorrector, PostCorrector};
pub use format::{SrtWriter, VttWriter, writer_for};
pub use layout::{LayoutRules, SpatialAssembler};
