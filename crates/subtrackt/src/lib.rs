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
pub mod pipeline;
pub mod report;

pub use config::{Config, UnmatchedPolicy};
pub use pipeline::{Outcome, Pipeline};
pub use report::Report;

pub use subtrackt_core as core;
pub use subtrackt_core::{Error, Result};
