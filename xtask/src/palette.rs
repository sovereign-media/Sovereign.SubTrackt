//! What a disc's subtitle palette actually holds.
//!
//! [#234](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/234). `binarize.rs` opens
//! by saying that classifying palette indices into fill, outline and anti-aliased edge is #5, and
//! #5 is closed with that sentence still true. `Threshold::default` still describes its own values
//! as *"a starting point, not a measured answer"*. Every glyph this pipeline has ever read was cut
//! at `min_luma: 128` with the outline discarded, and **nothing in this repository could print what
//! was being cut**.
//!
//! Two measurements have now wanted it. `docs/glyph-stability.md` refused palette-adaptive
//! thresholding on the grounds that *"subtitle palettes put fill near luma 235 and outline near 16,
//! so a fixed 128 is already comfortably in the gap"* — a claim about palettes, made without one
//! being counted. And #235's grey coverage collapses on VOBSUB, with a four-entry palette as the
//! leading explanation and no way to check.
//!
//! ## What it reports, and why by ink rather than by entry
//!
//! A palette declares up to 256 entries and a subtitle draws a handful. Reporting the declaration
//! would describe the format; reporting the entries that **cover pixels**, weighted by how many,
//! describes the disc. Every share below is of drawn foreground ink, so a title that authors sixteen
//! anti-aliasing levels and uses two of them reads as two.
//!
//! The classification is deliberately crude and deliberately named. `fill` is what the shipped
//! threshold keeps, `outline` is opaque ink below it, and `edge` is anything partially transparent
//! — those are the three the binarizer's own doc names, and the point of the survey is to say
//! whether a disc draws them as three separable clusters or as something else.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Context as _;
use subtrackt_core::{Palette, SubtitleImage};

/// The luma at or above which the shipped threshold calls an opaque pixel fill rather than outline.
///
/// `Threshold::default().min_luma`, duplicated here rather than widened into a public constant for
/// `geometry.rs`'s reason: this is a bench asking what that choice implies, not a second consumer
/// of it.
const MIN_LUMA: u8 = 128;

/// The alpha at or above which the shipped threshold calls a pixel foreground at all.
const MIN_ALPHA: u8 = 128;

/// One palette entry, and how much of the track it drew.
#[derive(Default, Clone, Copy)]
struct Drawn {
    pixels: u64,
    y: u8,
    alpha: u8,
}

/// Survey the palettes of one track.
///
/// # Errors
/// Fails if the media cannot be opened, no bitmap subtitle stream is found, or decoding fails.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: palette <media> [--stream N] [--cues N]")?
        .into();
    let stream = number(args, "--stream")?;
    let limit = number(args, "--cues")?.unwrap_or(usize::MAX);

    let mut source =
        subtrackt_demux::open(&media).with_context(|| format!("opening {}", media.display()))?;
    let streams = source.streams().to_vec();
    let chosen = match stream {
        Some(index) => streams
            .iter()
            .find(|s| s.index as usize == index)
            .context("no such stream")?
            .clone(),
        None => streams
            .first()
            .context("no bitmap subtitle stream")?
            .clone(),
    };
    let codec = chosen.codec.ffmpeg_name().to_owned();
    source
        .select(chosen.index)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut decoder = subtrackt_decode::decoder_for(&codec).map_err(|e| anyhow::anyhow!("{e}"))?;
    decoder
        .configure(&chosen.codec_private)
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // Keyed on the entry's *value* rather than its index, because PGS updates a palette
    // incrementally and VOBSUB carries one out of band: the same index can mean two colours in one
    // track, and two indices can mean one colour. What a survey is asking about is the colours.
    let mut drawn: BTreeMap<(u8, u8), Drawn> = BTreeMap::new();
    let (mut images, mut declared_max) = (0usize, 0usize);

    while images < limit {
        let Some(packet) = source.next_packet().map_err(|e| anyhow::anyhow!("{e}"))? else {
            break;
        };
        for image in decoder
            .push(packet.pts, &packet.payload)
            .map_err(|e| anyhow::anyhow!("{e}"))?
        {
            declared_max = declared_max.max(declared(&image.palette));
            tally(&image, &mut drawn);
            images += 1;
            if images >= limit {
                break;
            }
        }
    }

    report(&media, &codec, images, declared_max, &drawn);
    Ok(())
}

/// Add one image's pixels to the running tally.
fn tally(image: &SubtitleImage, drawn: &mut BTreeMap<(u8, u8), Drawn>) {
    let mut per_index = [0u64; 256];
    for &index in image.bitmap.pixels() {
        per_index[index as usize] += 1;
    }
    for (index, count) in per_index.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        #[allow(clippy::cast_possible_truncation)]
        let entry = image.palette.get(index as u8);
        // Fully transparent pixels are the background the format pads with, and counting them would
        // make every share a statement about how much empty plane a cue was composed onto.
        if entry.alpha == 0 {
            continue;
        }
        let slot = drawn.entry((entry.y, entry.alpha)).or_default();
        slot.pixels += count;
        slot.y = entry.y;
        slot.alpha = entry.alpha;
    }
}

/// How many entries of a palette carry any opacity at all.
fn declared(palette: &Palette) -> usize {
    (0..=u8::MAX).filter(|i| palette.get(*i).alpha > 0).count()
}

/// Which of the binarizer's three names an entry falls under, at the shipped threshold.
fn band(entry: Drawn) -> &'static str {
    if entry.alpha < MIN_ALPHA {
        "edge"
    } else if entry.y >= MIN_LUMA {
        "fill"
    } else {
        "outline"
    }
}

// Every figure here divides one pixel count by another. A feature film's subtitle track holds tens
// of millions, far inside the 2^53 an f64 counts exactly -- the same reasoning `geometry.rs`
// records for the same allow.
#[allow(clippy::cast_precision_loss)]
fn report(
    media: &std::path::Path,
    codec: &str,
    images: usize,
    declared_max: usize,
    drawn: &BTreeMap<(u8, u8), Drawn>,
) {
    let total: u64 = drawn.values().map(|d| d.pixels).sum();
    println!("\n--- palette survey (#234) ---");
    println!("  {} [{codec}], {images} images", media.display());
    println!(
        "  {} entries drawn of {declared_max} the palette declares opaque",
        drawn.len()
    );
    if total == 0 {
        println!("  no ink at all");
        return;
    }

    println!("\n  {:>5} {:>6} {:>12} {:>8}  band", "luma", "alpha", "pixels", "share");
    let mut ranked: Vec<&Drawn> = drawn.values().collect();
    ranked.sort_by_key(|d| std::cmp::Reverse(d.pixels));
    for entry in ranked.iter().take(16) {
        println!(
            "  {:>5} {:>6} {:>12} {:>7.2}%  {}",
            entry.y,
            entry.alpha,
            entry.pixels,
            entry.pixels as f64 * 100.0 / total as f64,
            band(**entry)
        );
    }
    if ranked.len() > 16 {
        let rest: u64 = ranked[16..].iter().map(|d| d.pixels).sum();
        println!(
            "  {:>5} {:>6} {:>12} {:>7.2}%  {} more entries",
            "",
            "",
            rest,
            rest as f64 * 100.0 / total as f64,
            ranked.len() - 16
        );
    }

    // The three shares the binarizer's own doc names, which is what a caller of this survey wants
    // to compare across discs.
    for name in ["fill", "outline", "edge"] {
        let share: u64 = drawn
            .values()
            .filter(|d| band(**d) == name)
            .map(|d| d.pixels)
            .sum();
        println!(
            "  {name:>8}: {:>7.2}% of drawn ink over {} entries",
            share as f64 * 100.0 / total as f64,
            drawn.values().filter(|d| band(**d) == name).count()
        );
    }

    // The claim `docs/glyph-stability.md` makes without having counted one: fill near 235, outline
    // near 16, so a fixed 128 sits comfortably in the gap. Printed as the widest empty band in the
    // luma distribution of *opaque* ink, which is the quantity that sentence is about.
    let mut opaque: Vec<u8> = drawn
        .values()
        .filter(|d| d.alpha >= MIN_ALPHA)
        .map(|d| d.y)
        .collect();
    opaque.sort_unstable();
    opaque.dedup();
    let gap = opaque
        .windows(2)
        .map(|pair| (pair[1] - pair[0], pair[0], pair[1]))
        .max();
    match gap {
        Some((width, low, high)) => println!(
            "  widest empty luma band in opaque ink: {low}..{high} ({width} wide); the shipped cut \
             is {MIN_LUMA}"
        ),
        None => println!("  opaque ink is a single luma; no band to cut"),
    }
}

fn number(args: &[String], flag: &str) -> anyhow::Result<Option<usize>> {
    match args.iter().position(|a| a == flag) {
        Some(at) => Ok(Some(
            args.get(at + 1)
                .with_context(|| format!("{flag} needs a value"))?
                .parse()
                .with_context(|| format!("{flag} takes a number"))?,
        )),
        None => Ok(None),
    }
}
