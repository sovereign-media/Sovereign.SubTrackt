//! Copying a container's PGS track out to a flat `.sup`.
//!
//! Not a feature — a `.sup` is not a subtitle anyone wants — but the thing that makes the disc
//! affordable to consult. `CLAUDE.md` says almost nothing here needs real media and
//! `docs/error-census.md` is the record of what happens when a proposal is ranked without it: every
//! accuracy finding since #98 has come from one Blu-ray, and that Blu-ray is 5.5 GB on a network
//! share. Reading it takes eight seconds; its subtitle track is sixteen megabytes and reads in a
//! tenth of one. A sweep that would have cost an hour of waiting costs a minute.
//!
//! The output is byte-for-byte the same track. `subtrackt_demux::sup` describes the format: a flat
//! sequence of segments, each behind a ten-byte header carrying the magic `PG`, a 90 kHz PTS and a
//! DTS. Matroska carries the same segments with no header at all, so this is a re-framing and not a
//! transcode — which is what makes it safe to measure against. The check that matters is not a unit
//! test but the whole track: extracting the 10 Cloverfield Lane rip and extracting its dump produce
//! **byte-identical** subtitles, and a bench quoting a figure from a dump should say so and be able to
//! show that.

use std::io::Write as _;
use std::path::PathBuf;

use anyhow::Context as _;

/// Bytes of segment header inside a container payload: type, then a big-endian length.
const SEGMENT_HEADER: usize = 3;

/// Split one container payload into the PGS segments it carries.
///
/// A Matroska block holds a whole display set — several segments concatenated — while a `.sup`
/// frames each one separately, so this is the only part of the copy that can be wrong. A payload
/// that does not divide exactly into segments is refused rather than truncated at the last whole
/// one: a short read here would drop the end of a display set and produce a track that decodes
/// cleanly and says something different.
fn segments(payload: &[u8]) -> anyhow::Result<Vec<(u8, &[u8])>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + SEGMENT_HEADER <= payload.len() {
        let kind = payload[at];
        let length = usize::from(u16::from_be_bytes([payload[at + 1], payload[at + 2]]));
        let end = at + SEGMENT_HEADER + length;
        let body = payload.get(at + SEGMENT_HEADER..end).with_context(|| {
            format!("a segment at byte {at} runs {length} bytes past the packet")
        })?;
        out.push((kind, body));
        at = end;
    }
    anyhow::ensure!(
        at == payload.len(),
        "{} bytes left over after the last whole segment",
        payload.len() - at
    );
    Ok(out)
}

/// Write one subtitle track out as a `.sup`.
///
/// # Errors
/// Fails if the source cannot be opened or read, if a packet does not divide into whole segments,
/// or if the output cannot be written.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let media: PathBuf = args
        .first()
        .context("usage: dump-sup <media> <out.sup> [--stream N]")?
        .into();
    let out: PathBuf = args.get(1).context("missing the output path")?.into();
    let wanted: Option<u32> = match args.iter().position(|a| a == "--stream") {
        Some(at) => Some(
            args.get(at + 1)
                .context("--stream needs a number")?
                .parse()?,
        ),
        None => None,
    };

    let mut source =
        subtrackt_demux::open(&media).with_context(|| format!("opening {}", media.display()))?;
    let stream = match wanted {
        Some(index) => source
            .streams()
            .iter()
            .find(|s| s.index == index)
            .with_context(|| format!("no subtitle stream with index {index}"))?
            .clone(),
        None => source
            .streams()
            .first()
            .context("no bitmap subtitle stream found")?
            .clone(),
    };
    anyhow::ensure!(
        stream.codec == subtrackt_demux::BitmapCodec::Pgs,
        "stream {} is {:?}; a .sup holds PGS and nothing else",
        stream.index,
        stream.codec
    );
    source.select(stream.index)?;

    let mut file = std::io::BufWriter::new(
        std::fs::File::create(&out).with_context(|| format!("creating {}", out.display()))?,
    );
    let (mut packets, mut written) = (0usize, 0usize);
    while let Some(packet) = source.next_packet()? {
        packets += 1;
        // A `.sup` PTS is 32 bits of 90 kHz ticks, which is thirteen hours. Saturating rather than
        // wrapping, because a wrapped timestamp would place a cue somewhere plausible.
        let pts = u32::try_from(packet.pts).unwrap_or(u32::MAX);
        for (kind, body) in segments(&packet.payload)
            .with_context(|| format!("packet {packets} at {} ticks", packet.pts))?
        {
            file.write_all(b"PG")?;
            file.write_all(&pts.to_be_bytes())?;
            // No decode timestamp. Nothing in this pipeline reads one, and inventing a value that
            // looked like a measurement would be worse than the zero that plainly is not.
            file.write_all(&0u32.to_be_bytes())?;
            file.write_all(&[kind])?;
            file.write_all(&u16::try_from(body.len())?.to_be_bytes())?;
            file.write_all(body)?;
            written += 1;
        }
    }
    file.flush()?;

    eprintln!(
        "stream {} ({:?}, {}x{}): {packets} packets, {written} segments -> {}",
        stream.index,
        stream.codec,
        stream.plane_width,
        stream.plane_height,
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(kind: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![kind];
        out.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn a_block_holding_a_whole_display_set_splits_into_its_segments() {
        let mut payload = segment(0x16, b"composition");
        payload.extend(segment(0x15, b"object"));
        payload.extend(segment(0x80, b""));

        let split = segments(&payload).unwrap();
        assert_eq!(split.len(), 3);
        assert_eq!(split[0], (0x16, b"composition".as_slice()));
        assert_eq!(split[1], (0x15, b"object".as_slice()));
        assert_eq!(split[2], (0x80, b"".as_slice()), "an end segment carries no body");
    }

    #[test]
    fn a_segment_running_past_the_packet_is_rejected_rather_than_truncated() {
        let mut payload = segment(0x15, b"object");
        payload.pop();
        assert!(
            segments(&payload).is_err(),
            "half an object would decode cleanly and say something else"
        );
    }

    #[test]
    fn trailing_bytes_that_are_not_a_whole_segment_are_rejected() {
        let mut payload = segment(0x16, b"composition");
        payload.extend_from_slice(&[0x15, 0x00]);
        assert!(segments(&payload).is_err());
    }

    #[test]
    fn an_empty_packet_holds_no_segments_rather_than_failing() {
        assert!(segments(&[]).unwrap().is_empty());
    }
}
