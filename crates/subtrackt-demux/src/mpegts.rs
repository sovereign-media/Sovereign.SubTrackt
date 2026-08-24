//! MPEG transport streams, and the Blu-ray `.m2ts` variant of one.
//!
//! Scoped by the same measurement [`crate::matroska`] was, from the other end. The library survey
//! found 1,326 of 1,328 titles carrying bitmap subtitles inside Matroska — so a native Matroska
//! parser covers 99.85% and this covers most of what is left. What justified writing it is the rule
//! #86 set for itself: not the share, but **an actual file that fails**. There is one, a 26.8 GB
//! `.m2ts`, and until this it came back as a tracking issue number.
//!
//! It is also the form a Blu-ray is actually in before anyone remuxes it, so a consumer feeding
//! discs rather than rips meets this on the first file rather than on the 0.15%.
//!
//! No dependency, for the reason `CLAUDE.md` gives: `ffmpeg-next` links system libraries and would
//! cost the single static binary #1 asks for. A transport stream is 188-byte packets with a
//! four-byte header, which is not this project's problem domain but is not DEFLATE either.
//!
//! Everything streams, as it must: the file above is 26.8 GB and its subtitle track is a few
//! megabytes inside it.
//!
//! # What is not here
//!
//! **MP4.** There are zero `.mp4` files carrying bitmap subtitles in the surveyed library and the
//! sample table is a separate job, so [`crate::container`] still refuses those by name. Scoping to
//! what there is evidence for is the same choice #4 made.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use subtrackt_core::{Error, Result};

use crate::{BitmapCodec, Packet, StreamInfo, SubtitleSource};

/// Bytes in a transport packet.
const TS_PACKET: usize = 188;

/// Bytes in a Blu-ray transport packet: a four-byte arrival-time header, then a transport packet.
///
/// The header is a copy timestamp the player uses for rate control and nothing here wants, so the
/// only thing this size costs is knowing which of the two a file is.
const M2TS_PACKET: usize = 192;

/// First byte of every transport packet.
const SYNC: u8 = 0x47;

/// The Program Association Table always rides here.
const PAT_PID: u16 = 0x0000;

/// Stream type of a Blu-ray Presentation Graphic Stream, as the PMT declares it.
const STREAM_TYPE_PGS: u8 = 0x90;

/// How far into the file the tables are looked for.
///
/// A PAT and a PMT are repeated every few hundred milliseconds and sit at the very front, so this
/// is generous by orders of magnitude. It is a bound rather than an estimate: without one, a file
/// that is not a transport stream at all would be read to its end before saying so.
const TABLE_SCAN_BYTES: u64 = 8 << 20;

/// One elementary stream carrying subtitles.
#[derive(Debug, Clone)]
struct Track {
    /// The packet identifier its packets carry, which is not the index exposed to a caller.
    pid: u16,
    info: StreamInfo,
}

/// A PES packet being reassembled across transport packets.
#[derive(Debug, Default)]
struct Pending {
    pts: u64,
    payload: Vec<u8>,
    /// Bytes the PES header declared, or `None` where it declared zero — legal, and meaning "until
    /// the next one starts".
    declared: Option<usize>,
    open: bool,
}

/// Reads bitmap subtitle packets out of a transport stream.
pub struct MpegTsReader<R> {
    inner: R,
    path: PathBuf,
    /// 188 for a broadcast stream, 192 for a Blu-ray one.
    stride: usize,
    /// Bytes of arrival-time header before the transport packet inside each stride: 0 or 4.
    ///
    /// Not the same as where the *sync byte* was found, and conflating the two was the first bug
    /// here: a Blu-ray packet starts four bytes before its sync, so seeking to the sync and then
    /// skipping the header again lands four bytes into the packet.
    header: usize,
    /// Absolute offset of the first packet, header included, so `select` can rewind.
    start: u64,
    tracks: Vec<Track>,
    streams: Vec<StreamInfo>,
    selected: Option<u16>,
    pending: Pending,
    finished: bool,
    /// One packet's bytes, reused.
    ///
    /// A 26.8 GB Blu-ray is 140 million transport packets and all but a fraction of a percent of
    /// them are video. Allocating per packet is the mistake #146 measured in the Matroska reader
    /// and it is worse here, because there is no seeking past anything — every packet is read. The
    /// payload is copied only once the PID is known to be the one wanted.
    buffer: Vec<u8>,
}

impl MpegTsReader<BufReader<File>> {
    /// Open a transport stream and read its tables.
    ///
    /// # Errors
    /// Returns [`Error::Io`] if the file cannot be read, and [`Error::Demux`] if it is not a
    /// transport stream or declares no bitmap subtitle track.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let file = File::open(path).map_err(|e| Error::io(path, e))?;
        // The same large buffer `matroska::open` uses, for the same reason: this walks the whole
        // file sequentially and will usually be pointed at a network filesystem.
        Self::from_reader(BufReader::with_capacity(1 << 20, file), path.to_path_buf())
    }
}

impl<R: Read + Seek> MpegTsReader<R> {
    /// Parse the tables from an already-open reader.
    ///
    /// # Errors
    /// As [`Self::open`].
    pub fn from_reader(inner: R, path: PathBuf) -> Result<Self> {
        let mut reader = Self {
            inner,
            path,
            stride: TS_PACKET,
            header: 0,
            start: 0,
            tracks: Vec::new(),
            streams: Vec::new(),
            selected: None,
            pending: Pending::default(),
            finished: false,
            buffer: vec![0; M2TS_PACKET],
        };
        reader.detect_layout()?;
        reader.read_tables()?;

        if reader.tracks.is_empty() {
            return Err(Error::Demux(format!(
                "{} declares no PGS subtitle track",
                reader.path.display()
            )));
        }
        reader.streams = reader.tracks.iter().map(|t| t.info.clone()).collect();
        Ok(reader)
    }

    /// Work out whether packets are 188 or 192 bytes, and where within one the sync byte is.
    ///
    /// By finding a run of sync bytes at a fixed stride rather than by trusting the extension. A
    /// `.ts` produced by remuxing a Blu-ray keeps the 192-byte packets, and a `.m2ts` that has been
    /// stripped of its arrival-time headers is 188 — the bytes are the only thing that knows.
    fn detect_layout(&mut self) -> Result<()> {
        let mut window = vec![0u8; M2TS_PACKET * 8];
        self.inner
            .seek(SeekFrom::Start(0))
            .map_err(|e| self.io(e))?;
        let read = read_up_to(&mut self.inner, &mut window).map_err(|e| self.io(e))?;
        let window = &window[..read];

        // Every layout is scored over the whole window and the best one wins, rather than the first
        // that matches. A single `0x47` is just a byte and two can coincide — a first-match scan
        // picks whichever coincidence it meets first, and on a Blu-ray stream that is a byte inside
        // packet one rather than the sync of packet two.
        //
        // A layout scores the number of consecutive packets whose sync byte is where it should be.
        // The real one scores every packet in the window; a coincidence scores two or three.
        let mut best: Option<(usize, usize, usize)> = None;
        for stride in [TS_PACKET, M2TS_PACKET] {
            for offset in 0..stride.min(window.len()) {
                let run = (0..=window.len() / stride)
                    .take_while(|k| {
                        window
                            .get(offset + k * stride)
                            .is_some_and(|byte| *byte == SYNC)
                    })
                    .count();
                // Two is the floor. One would match any `0x47` in the file, so a window holding a
                // single packet is refused rather than guessed at.
                if run >= 2 && best.is_none_or(|(seen, _, _)| run > seen) {
                    best = Some((run, stride, offset));
                }
            }
        }
        if let Some((_, stride, offset)) = best {
            self.stride = stride;
            self.header = stride - TS_PACKET;
            // The sync byte is `header` bytes into its packet, so the packet begins before it.
            self.start = (offset - self.header) as u64;
            return Ok(());
        }
        Err(Error::Demux(format!(
            "{} does not look like a transport stream: no run of sync bytes at 188 or 192",
            self.path.display()
        )))
    }

    fn io(&self, source: std::io::Error) -> Error {
        Error::io(&self.path, source)
    }

    /// Read the next transport packet into [`Self::buffer`], or `None` at end of file.
    ///
    /// Returns the PID, whether a new unit starts here, and where the payload sits in the buffer —
    /// so a caller that does not want this packet has copied nothing.
    fn next_ts(&mut self) -> Result<Option<(u16, bool, std::ops::Range<usize>)>> {
        let stride = self.stride;
        let read = read_up_to(&mut self.inner, &mut self.buffer[..stride])
            .map_err(|e| Error::io(&self.path, e))?;
        if read < stride {
            return Ok(None);
        }
        let packet = &self.buffer[self.header..stride];
        if packet[0] != SYNC {
            // Losing sync mid-file is a truncated or spliced recording. Stopping is the same answer
            // `sup::read_segment` gives: what came before is still good.
            return Ok(None);
        }

        let pid = (u16::from(packet[1] & 0x1F) << 8) | u16::from(packet[2]);
        let unit_start = packet[1] & 0x40 != 0;
        let control = (packet[3] >> 4) & 0x3;

        let mut at = 4;
        if control & 0b10 != 0 {
            // An adaptation field declares its own length, which may be the whole rest of the
            // packet — a stuffing packet with no payload at all.
            at = 5 + usize::from(packet[4]);
        }
        if control & 0b01 == 0 || at >= packet.len() {
            return Ok(Some((pid, unit_start, 0..0)));
        }
        let base = self.header;
        Ok(Some((pid, unit_start, base + at..stride)))
    }

    /// Read the PAT and every PMT it names, collecting the PGS tracks.
    fn read_tables(&mut self) -> Result<()> {
        self.inner
            .seek(SeekFrom::Start(self.start))
            .map_err(|e| self.io(e))?;

        let mut program_pids: Vec<u16> = Vec::new();
        let mut index = 0u32;
        let mut read = 0u64;

        while read < TABLE_SCAN_BYTES {
            let Some((pid, unit_start, range)) = self.next_ts()? else {
                break;
            };
            read += self.stride as u64;
            if !unit_start || range.is_empty() {
                continue;
            }
            let payload = &self.buffer[range];
            // A table's payload opens with a pointer to where its section starts.
            let start = 1 + usize::from(payload[0]);
            let Some(section) = payload.get(start..) else {
                continue;
            };

            if pid == PAT_PID {
                program_pids = parse_pat(section);
            } else if program_pids.contains(&pid) {
                for (stream_type, elementary) in parse_pmt(section) {
                    if stream_type != STREAM_TYPE_PGS
                        || self.tracks.iter().any(|t| t.pid == elementary)
                    {
                        continue;
                    }
                    self.tracks.push(Track {
                        pid: elementary,
                        info: StreamInfo {
                            index,
                            codec: BitmapCodec::Pgs,
                            // A transport stream carries no language for a Blu-ray subtitle track:
                            // it lives in the playlist beside the stream, not in the stream.
                            language: None,
                            title: None,
                            // Nor the subtitle plane. PGS declares it in every composition segment,
                            // so it is known a packet later and not a packet sooner — reporting a
                            // guess here would be inventing one.
                            plane_width: 0,
                            plane_height: 0,
                            codec_private: Vec::new(),
                        },
                    });
                    index += 1;
                }
            }
            // Every PGS track is declared in one PMT, so there is nothing to gain by reading on
            // once one has been seen.
            if !self.tracks.is_empty() {
                break;
            }
        }
        Ok(())
    }

    /// Take one transport packet's payload into the PES being assembled.
    ///
    /// Returns the packet if that payload completed one.
    fn feed(&mut self, unit_start: bool, payload: &[u8]) -> Option<Packet> {
        let mut finished = None;
        if unit_start {
            finished = self.close_pending();
            match parse_pes_header(payload) {
                Some((pts, declared, body)) => {
                    self.pending = Pending { pts, payload: body.to_vec(), declared, open: true };
                }
                // A unit that does not start with a PES header is not one this reader can place.
                None => self.pending = Pending::default(),
            }
        } else if self.pending.open {
            self.pending.payload.extend_from_slice(payload);
        }

        // A PES that declared its length is complete the moment it has that many bytes; one that
        // declared zero runs until the next unit starts, which the branch above catches.
        if let Some(declared) = self.pending.declared
            && self.pending.payload.len() >= declared
        {
            self.pending.payload.truncate(declared);
            finished = finished.or_else(|| self.close_pending());
        }
        finished
    }

    /// Close whatever PES is open and hand it back, if it holds anything.
    fn close_pending(&mut self) -> Option<Packet> {
        let pending = std::mem::take(&mut self.pending);
        (pending.open && !pending.payload.is_empty())
            .then_some(Packet { pts: pending.pts, payload: pending.payload })
    }
}

/// Read until the buffer is full or the reader is exhausted, returning how much was read.
///
/// `read_exact` cannot be used: the last packet of a file is routinely short, and that is an end of
/// stream rather than a failure.
fn read_up_to<R: Read>(reader: &mut R, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(filled)
}

/// The program-map PIDs a Program Association Table names.
fn parse_pat(section: &[u8]) -> Vec<u16> {
    let Some(body) = table_body(section, 0x00) else {
        return Vec::new();
    };
    // Five bytes of table header, then pairs of program number and map PID, then a CRC.
    body.get(5..)
        .unwrap_or_default()
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|entry| u16::from_be_bytes([entry[0], entry[1]]) != 0)
        .map(|entry| (u16::from(entry[2] & 0x1F) << 8) | u16::from(entry[3]))
        .collect()
}

/// The `(stream type, elementary PID)` pairs a Program Map Table names.
fn parse_pmt(section: &[u8]) -> Vec<(u8, u16)> {
    let Some(body) = table_body(section, 0x02) else {
        return Vec::new();
    };
    // Five bytes of table header, two of PCR PID, two of program-info length, then descriptors.
    let Some(info_len) = body
        .get(7..9)
        .map(|b| usize::from(u16::from_be_bytes([b[0] & 0x0F, b[1]])))
    else {
        return Vec::new();
    };
    let mut at = 9 + info_len;
    let mut out = Vec::new();
    while at + 5 <= body.len() {
        let stream_type = body[at];
        let pid = (u16::from(body[at + 1] & 0x1F) << 8) | u16::from(body[at + 2]);
        let es_len = usize::from(u16::from_be_bytes([body[at + 3] & 0x0F, body[at + 4]]));
        out.push((stream_type, pid));
        at += 5 + es_len;
    }
    out
}

/// A section's body, checked against the table it claims to be and trimmed to its declared length.
///
/// The four trailing CRC bytes are dropped rather than verified. A corrupt section here yields a
/// track list that is wrong in a way the PGS decoder then rejects loudly, which is the same
/// division of labour `matroska` makes with its zlib checksum.
fn table_body(section: &[u8], table_id: u8) -> Option<&[u8]> {
    if *section.first()? != table_id {
        return None;
    }
    let length = usize::from(u16::from_be_bytes([section.get(1)? & 0x0F, *section.get(2)?]));
    let body = section.get(3..3 + length)?;
    body.get(..body.len().checked_sub(4)?)
}

/// The presentation timestamp, declared length and payload of a PES packet.
///
/// `None` for anything that is not a PES packet with a timestamp — which for a subtitle track means
/// a packet this reader cannot place in time, and placing it wrongly would be worse than dropping
/// it.
fn parse_pes_header(payload: &[u8]) -> Option<(u64, Option<usize>, &[u8])> {
    if payload.get(..3)? != [0x00, 0x00, 0x01] {
        return None;
    }
    let declared = usize::from(u16::from_be_bytes([*payload.get(4)?, *payload.get(5)?]));
    let flags = *payload.get(7)?;
    let header_len = usize::from(*payload.get(8)?);
    if flags & 0x80 == 0 {
        return None;
    }
    let stamp = payload.get(9..14)?;
    let pts = (u64::from(stamp[0] & 0x0E) << 29)
        | (u64::from(stamp[1]) << 22)
        | (u64::from(stamp[2] & 0xFE) << 14)
        | (u64::from(stamp[3]) << 7)
        | (u64::from(stamp[4]) >> 1);

    let body = payload.get(9 + header_len..)?;
    // The declared length counts the three bytes of flags and header length as well as the payload,
    // and zero is legal — it means "until the next PES starts", which only video uses in practice.
    let remaining = declared.checked_sub(3 + header_len).filter(|n| *n > 0);
    Some((pts, remaining, body))
}

impl<R: Read + Seek> SubtitleSource for MpegTsReader<R> {
    fn streams(&self) -> &[StreamInfo] {
        &self.streams
    }

    fn select(&mut self, index: u32) -> Result<()> {
        let track = self
            .tracks
            .iter()
            .find(|t| t.info.index == index)
            .ok_or_else(|| Error::Demux(format!("no subtitle stream with index {index}")))?;
        self.selected = Some(track.pid);
        self.pending = Pending::default();
        self.finished = false;
        self.inner
            .seek(SeekFrom::Start(self.start))
            .map_err(|e| self.io(e))?;
        Ok(())
    }

    fn next_packet(&mut self) -> Result<Option<Packet>> {
        if self.selected.is_none() {
            let first = self.streams.first().map(|s| s.index);
            match first {
                Some(index) => self.select(index)?,
                None => return Ok(None),
            }
        }
        let wanted = self.selected.unwrap_or_default();

        loop {
            let Some((pid, unit_start, range)) = self.next_ts()? else {
                // End of file closes whatever was still open, which is where the last cue of a
                // stream that ends mid-packet comes from.
                if self.finished {
                    return Ok(None);
                }
                self.finished = true;
                return Ok(self.close_pending());
            };
            if pid != wanted || range.is_empty() {
                continue;
            }
            // Copied only now that the PID is known to be the one wanted, which on a Blu-ray is a
            // fraction of a percent of the packets read.
            let payload = self.buffer[range].to_vec();
            if let Some(packet) = self.feed(unit_start, &payload) {
                return Ok(Some(packet));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// One transport packet carrying `payload`, padded with an adaptation field if it is short.
    fn ts(pid: u16, unit_start: bool, payload: &[u8]) -> Vec<u8> {
        let mut packet = vec![SYNC, 0, 0, 0];
        packet[1] = u8::try_from(pid >> 8).unwrap() & 0x1F | u8::from(unit_start) << 6;
        packet[2] = u8::try_from(pid & 0xFF).unwrap();
        let room = TS_PACKET - 4;
        if payload.len() < room {
            // Adaptation field first, then payload: control bits 0b11.
            packet[3] = 0x30;
            let stuffing = room - payload.len() - 1;
            packet.push(u8::try_from(stuffing).unwrap());
            packet.extend(std::iter::repeat_n(0xFFu8, stuffing));
        } else {
            packet[3] = 0x10;
        }
        packet.extend_from_slice(&payload[..payload.len().min(TS_PACKET - packet.len())]);
        packet.resize(TS_PACKET, 0xFF);
        packet
    }

    /// A section with its pointer field, table id and length.
    fn section(table_id: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![0x00, table_id];
        let length = u16::try_from(body.len() + 4).unwrap() | 0xB000;
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out
    }

    fn pat(program_pid: u16) -> Vec<u8> {
        let mut body = vec![0, 1, 0xC1, 0, 0];
        body.extend_from_slice(&1u16.to_be_bytes());
        body.extend_from_slice(&(program_pid | 0xE000).to_be_bytes());
        section(0x00, &body)
    }

    fn pmt(entries: &[(u8, u16)]) -> Vec<u8> {
        let mut body = vec![0, 1, 0xC1, 0, 0];
        body.extend_from_slice(&0xE100u16.to_be_bytes());
        body.extend_from_slice(&0xF000u16.to_be_bytes());
        for (stream_type, pid) in entries {
            body.push(*stream_type);
            body.extend_from_slice(&(pid | 0xE000).to_be_bytes());
            body.extend_from_slice(&0xF000u16.to_be_bytes());
        }
        section(0x02, &body)
    }

    /// A PES packet carrying `payload` at `pts`.
    fn pes(pts: u64, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0x00, 0x00, 0x01, 0xBD];
        let declared = u16::try_from(payload.len() + 8).unwrap();
        out.extend_from_slice(&declared.to_be_bytes());
        out.extend_from_slice(&[0x80, 0x80, 5]);
        out.push(0x21 | u8::try_from((pts >> 29) & 0x0E).unwrap());
        out.push(u8::try_from((pts >> 22) & 0xFF).unwrap());
        out.push(u8::try_from(((pts >> 14) & 0xFE) | 1).unwrap());
        out.push(u8::try_from((pts >> 7) & 0xFF).unwrap());
        out.push(u8::try_from(((pts << 1) & 0xFE) | 1).unwrap());
        out.extend_from_slice(payload);
        out
    }

    fn stream(subtitle_pid: u16, packets: &[(u64, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(ts(PAT_PID, true, &pat(0x0100)));
        out.extend(ts(
            0x0100,
            true,
            &pmt(&[(0x1B, 0x1011), (STREAM_TYPE_PGS, subtitle_pid)]),
        ));
        for (pts, payload) in packets {
            out.extend(ts(subtitle_pid, true, &pes(*pts, payload)));
        }
        out
    }

    fn open(bytes: Vec<u8>) -> Result<MpegTsReader<Cursor<Vec<u8>>>> {
        MpegTsReader::from_reader(Cursor::new(bytes), "test.ts".into())
    }

    #[test]
    fn a_pgs_track_is_found_through_the_pat_and_the_pmt() {
        let reader = open(stream(0x1220, &[(90_000, vec![1, 2, 3])])).unwrap();
        assert_eq!(reader.streams().len(), 1);
        assert_eq!(reader.streams()[0].codec, BitmapCodec::Pgs);
        // Neither is in a transport stream, and saying so beats guessing. See `read_tables`.
        assert_eq!(reader.streams()[0].language, None);
        assert_eq!(reader.streams()[0].plane_width, 0);
    }

    #[test]
    fn a_stream_carrying_only_video_is_refused_rather_than_read_as_empty() {
        // The distinction #1 rests on: a file with no subtitle track is a fact a caller can act on,
        // and a reader that returned no packets would look exactly like one that read a whole track
        // and found nothing in it.
        let mut bytes = Vec::new();
        bytes.extend(ts(PAT_PID, true, &pat(0x0100)));
        bytes.extend(ts(0x0100, true, &pmt(&[(0x1B, 0x1011), (0x81, 0x1100)])));
        assert!(matches!(open(bytes), Err(Error::Demux(_))));
    }

    #[test]
    fn a_file_that_is_not_a_transport_stream_says_so() {
        assert!(matches!(open(vec![0x00; 4096]), Err(Error::Demux(_))));
    }

    #[test]
    fn packets_come_back_with_the_timestamp_their_pes_header_carried() {
        let mut reader = open(stream(
            0x1220,
            &[
                (90_000, vec![0x16, 0, 1, 0xAA]),
                (180_000, vec![0x80, 0, 0]),
            ],
        ))
        .unwrap();
        let first = reader.next_packet().unwrap().unwrap();
        assert_eq!(first.pts, 90_000);
        assert_eq!(first.payload, vec![0x16, 0, 1, 0xAA]);
        let second = reader.next_packet().unwrap().unwrap();
        assert_eq!(second.pts, 180_000);
        assert!(reader.next_packet().unwrap().is_none());
    }

    #[test]
    fn a_pes_spanning_several_transport_packets_is_reassembled() {
        // The case that makes this a reader rather than a loop: a display set with a large object
        // is thousands of bytes and a transport packet holds 184.
        let payload: Vec<u8> = (0..600u32)
            .map(|i| u8::try_from(i % 251).unwrap())
            .collect();
        let whole = pes(90_000, &payload);

        let mut bytes = Vec::new();
        bytes.extend(ts(PAT_PID, true, &pat(0x0100)));
        bytes.extend(ts(0x0100, true, &pmt(&[(STREAM_TYPE_PGS, 0x1220)])));
        let mut at = 0;
        let mut first = true;
        while at < whole.len() {
            let take = (TS_PACKET - 4).min(whole.len() - at);
            bytes.extend(ts(0x1220, first, &whole[at..at + take]));
            at += take;
            first = false;
        }

        let mut reader = open(bytes).unwrap();
        let packet = reader.next_packet().unwrap().unwrap();
        assert_eq!(packet.pts, 90_000);
        assert_eq!(packet.payload, payload, "the pieces must come back in order and whole");
    }

    #[test]
    fn a_bluray_packet_is_read_through_its_arrival_time_header() {
        // A `.m2ts` prefixes every packet with four bytes the player uses for rate control. The
        // extension does not decide this — a `.ts` remuxed from a disc keeps them — so the layout
        // is found in the bytes.
        let plain = stream(0x1220, &[(90_000, vec![0x16, 0, 1, 0xAA])]);
        let mut wrapped = Vec::new();
        for packet in plain.chunks(TS_PACKET) {
            wrapped.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
            wrapped.extend_from_slice(packet);
        }

        let mut reader = open(wrapped).unwrap();
        assert_eq!(reader.stride, M2TS_PACKET);
        assert_eq!(reader.header, 4);
        let packet = reader.next_packet().unwrap().unwrap();
        assert_eq!(packet.pts, 90_000);
        assert_eq!(packet.payload, vec![0x16, 0, 1, 0xAA]);
    }

    #[test]
    fn selecting_a_stream_that_does_not_exist_is_refused() {
        let mut reader = open(stream(0x1220, &[(0, vec![1])])).unwrap();
        assert!(reader.select(7).is_err());
    }

    #[test]
    fn a_second_pass_over_the_same_track_reads_the_same_packets() {
        // `select` rewinds, which is what lets a caller list the streams and then read one.
        let bytes = stream(0x1220, &[(90_000, vec![1, 2]), (180_000, vec![3, 4])]);
        let mut reader = open(bytes).unwrap();
        let first: Vec<_> = std::iter::from_fn(|| reader.next_packet().unwrap()).collect();
        reader.select(0).unwrap();
        let second: Vec<_> = std::iter::from_fn(|| reader.next_packet().unwrap()).collect();
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
    }
}
