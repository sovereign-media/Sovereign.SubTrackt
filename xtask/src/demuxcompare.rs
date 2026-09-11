//! Reading one Matroska track both ways and holding the index to the walk.
//!
//! #258 taught the reader to find a track's blocks through the file's own `Cues` rather than by
//! walking every cluster. The two have to yield the same packets — the same timestamps and the
//! same bytes — and this is where that is checked on real media rather than on a fixture. It is
//! also the instrument a library survey needs: the one risk the index carries is a muxer that
//! entered some blocks and not others, and nothing in the file says so. Only a walk can.
//!
//! The index runs first, so its time is not flattered by a cache the walk warmed. A walk of a
//! remux is tens of gigabytes and warms nothing the index would read anyway, but the order costs
//! nothing and removes the question.
//!
//! ```console
//! $ cargo run -p xtask --release -- demux-compare film.mkv --stream 0
//! ```

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Context as _;
use subtrackt_demux::matroska::MatroskaReader;
use subtrackt_demux::{Packet, SubtitleSource as _};

/// One way of reading the track, and what it cost.
struct Run {
    packets: Vec<Packet>,
    seconds: f64,
    access: subtrackt_demux::Access,
}

fn read_all<R: Read + Seek>(mut reader: MatroskaReader<R>, stream: u32) -> anyhow::Result<Run> {
    let started = Instant::now();
    reader.select(stream)?;
    let mut packets = Vec::new();
    while let Some(packet) = reader.next_packet()? {
        packets.push(packet);
    }
    Ok(Run {
        packets,
        seconds: started.elapsed().as_secs_f64(),
        access: reader.access(),
    })
}

/// The reader as `subtrackt extract` opens it, or with the index path's handle opened without the
/// random-access hint, to price what the hint is worth.
fn open(path: &Path, hint: bool) -> anyhow::Result<MatroskaReader<BufReader<File>>> {
    if hint {
        return Ok(MatroskaReader::open(path)?);
    }
    let walk = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let random = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    Ok(
        MatroskaReader::from_reader(BufReader::with_capacity(1 << 20, walk), path.to_path_buf())?
            .with_random_access(random)?,
    )
}

/// Run the comparison.
///
/// # Errors
/// Fails if the file cannot be read, and if the two ways of reading it disagree.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: demux-compare <file.mkv> [--stream N] [--index-only] [--no-hint]")?
        .into();
    let stream: u32 = match args.iter().position(|a| a == "--stream") {
        Some(at) => args
            .get(at + 1)
            .context("--stream needs a number")?
            .parse()?,
        None => 0,
    };
    let index_only = args.iter().any(|a| a == "--index-only");
    let hint = !args.iter().any(|a| a == "--no-hint");

    let indexed = read_all(open(&media, hint)?, stream)?;
    println!(
        "index: {} packets in {:.2} s; {}",
        indexed.packets.len(),
        indexed.seconds,
        indexed.access
    );
    if index_only {
        return Ok(());
    }

    let mut reader = open(&media, hint)?;
    reader.use_index(false);
    let walked = read_all(reader, stream)?;
    println!(
        "walk:  {} packets in {:.2} s; {}",
        walked.packets.len(),
        walked.seconds,
        walked.access
    );

    if let Some(at) = (0..indexed.packets.len().max(walked.packets.len()))
        .find(|&i| indexed.packets.get(i) != walked.packets.get(i))
    {
        let describe = |p: Option<&Packet>| {
            p.map_or_else(
                || "nothing".to_owned(),
                |p| format!("pts {} ({} bytes)", p.pts, p.payload.len()),
            )
        };
        anyhow::bail!(
            "the two disagree at packet {at}: the index read {}, the walk {}",
            describe(indexed.packets.get(at)),
            describe(walked.packets.get(at)),
        );
    }
    println!("packets: identical ({})", walked.packets.len());
    Ok(())
}
