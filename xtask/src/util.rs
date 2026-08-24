//! Helpers more than one command needs.
//!
//! Small things, and they are here because they were written twice. #144 found `median` in two
//! files, `nearest` in two more with the same body and the same doc comment, `stem` in another two,
//! and twelve copies of loading a reference set from disk. None of that was a decision; it is what
//! a directory of thirty single-purpose commands accumulates.
//!
//! **Deliberately not the percentile family.** There are four spellings of that across `fontid`,
//! `slant`, `refmatch` and `stability`, and they use *three different index formulas* — so merging
//! them is not deduplication, it is choosing which published figure to change. That is its own
//! question and it needs its own measurement.

use std::path::Path;

use anyhow::Context as _;
use subtrackt_glyph::ReferenceSet;

/// Load a `.subtref` from disk.
///
/// Twelve call sites wrote this out, ten of them wrapping the parse in
/// `map_err(|e| anyhow::anyhow!("{e}"))`. That adapter was never needed — `subtrackt_core::Error`
/// converts through `?` on its own — and it was actively harmful: it flattens a structured error
/// into a string and drops its `source()`, so an `Error::Io` arrived without the OS error under it.
pub fn load_reference(path: &Path) -> anyhow::Result<ReferenceSet> {
    let bytes =
        std::fs::read(path).with_context(|| format!("reading reference set {}", path.display()))?;
    ReferenceSet::decode(&bytes)
        .with_context(|| format!("parsing reference set {}", path.display()))
}

/// The median of a list, taking the upper of the two middles on an even count.
pub fn median(values: &mut [u32]) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

/// A path's file stem, or `unnamed` for a path that has none.
pub fn stem(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| "unnamed".to_owned(), |s| s.to_string_lossy().into_owned())
}
