//! Extract plain text from bitmap image-based subtitle streams.
//!
//! This crate wires the stage crates into the pipeline from the architecture document and is the
//! only crate a caller needs. It is deliberately a library with a thin binary on top rather than a
//! binary with helper modules: whether this ships as a CLI, as a `cdylib` behind P/Invoke, or as
//! both is still open (#16), and that decision should cost a new crate rather than a rewrite.
//!
//! ```no_run
//! use subtrackt::{Config, Pipeline};
//!
//! let outcome = Pipeline::new(Config::default()).run("movie.sup")?;
//! println!("{} cues, {} unmatched glyphs", outcome.track.cues.len(), outcome.report.unmatched);
//! # Ok::<(), subtrackt::Error>(())
//! ```

pub mod config;
pub mod fit;
pub mod pipeline;
pub mod provenance;
pub mod report;
pub mod score;
pub mod survey;

pub use config::{Config, Defusing, ProvenancePolicy, UnmatchedPolicy};
pub use fit::{Fit, rank, rank_watched, score_set};
pub use pipeline::{Outcome, Pipeline, UnreadGlyph};
pub use provenance::note;
pub use report::{Cost, Report};
pub use score::{Score, score_text, score_track};
pub use subtrackt_text::correct::VocabularyRules;
pub use survey::{GlyphRecord, GlyphSurvey};

pub use subtrackt_core as core;
// Post-correction is the one stage whose output a caller has to be able to audit rather than
// merely count, so its log type is part of this crate's surface and not an implementation detail
// of `subtrackt-text`.
pub use subtrackt_core::progress::{Phase, Progress, Silent};
pub use subtrackt_core::{Error, Result};
pub use subtrackt_text::correct::{CorrectionLog, PostCorrector};
// `Config::layout` is a `LayoutRules`, so the type is already part of this crate's surface; naming
// it here saves every caller a dependency on `subtrackt-text` to construct one.
pub use subtrackt_text::layout::{LayoutRules, SpacingRule};
