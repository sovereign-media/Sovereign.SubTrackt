# Fixture provenance

## `synthetic.sup` / `synthetic.txt`

**Generated, not clipped.** Produced by `cargo run -p xtask -- make-fixture <font> <dir>`.

- **Text**: written for this repository. Not taken from any film, broadcast or published work.
- **Bitmap**: our own rasterisation of that text, encoded as PGS by `rle::encode`.
- **Rendering**: anti-aliased fill inside a one-pixel dark outline, imitating how a real subtitle is
  authored. That is deliberate — #14 measured that a one-pixel shift in the binarization edge costs
  as much as character identity, so a fixture of clean solid text would exercise an easier problem
  than the real one and flatter every stage downstream.
- **Font**: rasterised with Arial on the machine that generated it. Only the resulting bitmap is
  committed; no font program is redistributed, and the typeface design itself is not the subject of
  copyright in the jurisdictions that matter here. Regenerating with any other font produces an
  equally valid fixture, which is what `xtask accuracy` does at runtime.

Nothing in this directory is derived from copyrighted subtitle content, which is the whole reason
fixtures are generated rather than sampled.

## Why there is no VOBSUB fixture yet

#3 is unimplemented, so nothing can decode one to check a fixture against. Add one with #3.
