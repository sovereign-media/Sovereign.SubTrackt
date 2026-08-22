//! Reporting where a run has got to.
//!
//! A long run that says nothing is indistinguishable from a hung one, and the pipeline is where
//! the time goes. But it cannot draw anything: rendering belongs to whichever front end is
//! attached, and a library crate here takes no dependency that could draw. So this is the same
//! shape as [`crate::stage`] — a trait the caller implements — and the pipeline calls it without
//! knowing whether anything is listening.
//!
//! Nothing is by default. [`Silent`] is what a caller who did not ask gets, and every method on it
//! is empty, so the observed and unobserved paths are the same code.

/// Which part of a run is being reported.
///
/// Named for the work rather than for the function that does it, because the label is drawn on a
/// terminal for a person to read. Whether a phase is determinate is not a property of the phase —
/// it is [`Progress::begin`]'s `total` — because the same phase can be both: a survey of a whole
/// file has no end in sight, and a survey capped at four hundred cues does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Phase {
    /// Streaming packets out of the container and decoding them into bitmaps.
    Decode,
    /// Cutting bitmaps into glyphs.
    Segment,
    /// Grouping the stream's own shapes, before any of them is matched.
    Cluster,
    /// Matching glyphs and assembling them into cues.
    Read,
    /// Segmenting a file for its shapes alone, without trying to read them.
    Survey,
    /// Scoring candidate reference sets against a title.
    Score,
    /// Rasterising fonts into reference sets.
    Render,
}

impl Phase {
    /// What to call this phase on screen.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Decode => "decoding",
            Self::Segment => "segmenting",
            Self::Cluster => "clustering",
            Self::Read => "reading",
            Self::Survey => "surveying",
            Self::Score => "scoring",
            Self::Render => "rendering",
        }
    }

    /// What the position counts, so a bare number on screen says what it is a number of.
    #[must_use]
    pub const fn unit(self) -> &'static str {
        match self {
            Self::Decode => "packets",
            Self::Segment | Self::Read => "images",
            Self::Cluster => "shapes",
            Self::Survey => "cues",
            Self::Score => "sets",
            Self::Render => "fonts",
        }
    }
}

/// Something watching a run.
///
/// Calls arrive in the order `begin`, zero or more `advance`, `end`, and phases do not overlap —
/// the pipeline is sequential, so one phase is active at a time and an implementation needs no
/// stack.
///
/// Every method takes `&self`. An observer that has to mutate — a renderer tracking a spinner
/// frame — carries its own interior mutability, which keeps this usable from a `&self` pipeline
/// method without threading a borrow through every stage.
pub trait Progress {
    /// A phase started. `total` is the number of units it will cover, or `None` when that is not
    /// known until the phase ends.
    fn begin(&self, phase: Phase, total: Option<u64>);

    /// The phase has now covered `position` units, counted from the start of the phase.
    ///
    /// Absolute rather than a delta, so a caller that already has an index does not have to keep a
    /// second counter, and a dropped call cannot make the total drift.
    fn advance(&self, position: u64);

    /// The phase finished. For a determinate phase this follows an `advance` at the total: a bar
    /// that stops at 97% is a lie about where the work went.
    fn end(&self);
}

/// The observer a run has when nobody is watching.
#[derive(Debug, Clone, Copy, Default)]
pub struct Silent;

impl Progress for Silent {
    fn begin(&self, _phase: Phase, _total: Option<u64>) {}
    fn advance(&self, _position: u64) {}
    fn end(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phase_names_itself_and_what_it_counts() {
        // Both strings are drawn on a terminal, so an empty one is a blank column rather than a
        // compile error. Cheap to pin, and the enum is the kind of thing that grows a variant.
        for phase in [
            Phase::Decode,
            Phase::Segment,
            Phase::Cluster,
            Phase::Read,
            Phase::Survey,
            Phase::Score,
            Phase::Render,
        ] {
            assert!(!phase.label().is_empty(), "{phase:?}");
            assert!(!phase.unit().is_empty(), "{phase:?}");
        }
    }

    #[test]
    fn the_silent_observer_is_usable_as_a_trait_object() {
        // What `Pipeline::run` hands itself. If this stopped compiling the trait would have grown
        // something that cannot be dynamically dispatched, and the whole design goes with it.
        let observer: &dyn Progress = &Silent;
        observer.begin(Phase::Decode, None);
        observer.advance(1);
        observer.end();
    }
}
