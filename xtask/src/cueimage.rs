//! Writing out the bitmap of every cue, so a person can read what the pipeline read.
//!
//! [#185](https://github.com/sovereign-media/Sovereign.SubTrackt/issues/185). Every accuracy figure
//! in this repository is scored against a **release sidecar** — another transcript of the same
//! dialogue, frequently read off the same bitmaps by some other tool. `docs/post-correction.md`
//! names the hole that leaves: a systematic error the corrector introduces could in principle be
//! matched by the same systematic error in the comparison, and score as agreement.
//!
//! Closing it needs ground truth a person checked against the images, and checking against the
//! images needs the images. This is that: one grey PGM per cue, in cue order, with a manifest
//! naming each one's index and time span.
//!
//! **PGM rather than PNG**, and P5 rather than P2: a binary grey map is a header and the bytes,
//! which is why it needs no dependency and no encoder to be worth trusting. `scripts/truth/` turns
//! them into contact sheets.
//!
//! What is written is the **composed image**, not the binarizer's mask. The mask is what the
//! pipeline decided the ink was, and a ground truth authored from it would inherit exactly the
//! upstream decisions it exists to check.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use subtrackt_core::SubtitleImage;

/// Dump each cue's bitmap as a grey PGM.
///
/// # Errors
/// Fails if the media cannot be opened or decoded, or if a file cannot be written.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: cue-images <media> <out-dir> [--stream N] [--from N] [--count N]")?
        .into();
    let out: PathBuf = args.get(1).context("missing the output directory")?.into();
    let stream = number(args, "--stream")?;
    let from = number(args, "--from")?.unwrap_or(0);
    let count = number(args, "--count")?.unwrap_or(usize::MAX);

    std::fs::create_dir_all(&out).with_context(|| format!("creating {}", out.display()))?;

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
    source
        .select(chosen.index)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut decoder = subtrackt_decode::decoder_for(chosen.codec.ffmpeg_name())
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let manifest_path = out.join("cues.tsv");
    let mut manifest = std::io::BufWriter::new(
        std::fs::File::create(&manifest_path)
            .with_context(|| format!("creating {}", manifest_path.display()))?,
    );
    writeln!(manifest, "cue\tstart_ms\tend_ms\tfile")?;

    let mut index = 0usize;
    let mut written = 0usize;
    loop {
        let Some(packet) = source.next_packet().map_err(|e| anyhow::anyhow!("{e}"))? else {
            break;
        };
        let images = decoder
            .push(packet.pts, &packet.payload)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        for image in images {
            written += emit(&image, index, from, count, &out, &mut manifest)?;
            index += 1;
        }
        if written >= count {
            break;
        }
    }
    for image in decoder.finish().map_err(|e| anyhow::anyhow!("{e}"))? {
        written += emit(&image, index, from, count, &out, &mut manifest)?;
        index += 1;
    }

    manifest.flush()?;
    eprintln!("{written} cues written to {} ({index} decoded)", out.display());
    Ok(())
}

/// Write one cue if it is in range, and say whether it was.
fn emit(
    image: &SubtitleImage,
    index: usize,
    from: usize,
    count: usize,
    out: &Path,
    manifest: &mut impl std::io::Write,
) -> anyhow::Result<usize> {
    if index < from || index >= from.saturating_add(count) {
        return Ok(0);
    }
    let name = format!("cue-{index:05}.pgm");
    write_pgm(image, &out.join(&name))?;
    writeln!(
        manifest,
        "{index}\t{}\t{}\t{name}",
        image.span.start.as_millis(),
        image.span.end.as_millis()
    )?;
    Ok(1)
}

/// One image as a binary grey map, alpha composited over black.
///
/// Over black rather than over the plane, because a subtitle plane is transparent and there is no
/// video here to lay it on — and because black behind white text is what the ink looks like to the
/// binarizer, so a reader is looking at the same contrast the pipeline was.
fn write_pgm(image: &SubtitleImage, path: &Path) -> anyhow::Result<()> {
    let bitmap = &image.bitmap;
    let (width, height) = (bitmap.width(), bitmap.height());
    let mut bytes = Vec::with_capacity((width as usize * height as usize).saturating_add(32));
    bytes.extend_from_slice(format!("P5\n{width} {height}\n255\n").as_bytes());
    for pixel in bitmap.pixels() {
        let entry = image.palette.get(*pixel);
        let rgba = entry.to_rgba();
        // Grey by the same luma the palette carries, scaled by opacity. A fully transparent index
        // is background whatever its colour says, which is what `alpha` means.
        let luma = u32::from(rgba.r) * 299 + u32::from(rgba.g) * 587 + u32::from(rgba.b) * 114;
        #[allow(clippy::cast_possible_truncation)]
        let grey = ((luma / 1000) * u32::from(entry.alpha) / 255) as u8;
        bytes.push(grey);
    }
    std::fs::write(path, &bytes).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// A numeric argument, if present.
fn number(args: &[String], flag: &str) -> anyhow::Result<Option<usize>> {
    match args.iter().position(|a| a == flag) {
        Some(at) => Ok(Some(
            args.get(at + 1)
                .with_context(|| format!("{flag} needs a number"))?
                .parse()
                .with_context(|| format!("{flag} takes a number"))?,
        )),
        None => Ok(None),
    }
}
