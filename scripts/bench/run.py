#!/usr/bin/env python3
"""Run the standing accuracy bench, and price a change against it.

`roster.json` says which tracks the bench holds and why. This runs them.

Until #133 the bench was three discs recorded in prose, so pricing a change meant re-deriving the
shell by hand, reading tens of gigabytes over the network, and choosing sidecars afresh each time.
Nothing stopped the next measurement quietly using two discs instead of three -- and #113 exists
because a change was shipped on the evidence of one.

Three steps:

    dump      pull each track out of its container once, into a local `.sup`
    score     extract and score every track, into a results file
    compare   two results files, per track, through `xtask srt-score --compare`

Dumping is the slow part and is done once: about 30 minutes and 230 MB over the network for the
seven tracks. After that a whole bench pass is a few seconds, which is what makes running it before
*and* after a change the cheap option rather than the diligent one.

    $ scripts/bench/run.py dump    --cache bench-cache
    $ scripts/bench/run.py score   --cache bench-cache --reference arial-ri.subtref --out before.json
    $ ...make the change...
    $ scripts/bench/run.py score   --cache bench-cache --reference arial-ri.subtref --out after.json
    $ scripts/bench/run.py compare before.json after.json

**Read the `worse` column, not the CER.** A change can gain character error on one disc while making
hundreds of cues worse on another; #110 did exactly that, and #113 found it only because two more
discs were scored.

**A CER is refused before it is reported.** #229 found the bench's worst number -- King Kong at
21.3% -- was 529 of 1,704 cues pairing by start time onto a neighbour's text, because the disc is a
Blu-ray and its only English sidecar is a WEB-DL. The same track reads 6.0% with cue boundaries
ignored. `PAIRED_ENOUGH` is the check that catches the next one, and the unpaired count now reaches
the results file for every entry rather than scrolling past on stderr.
"""

import argparse
import ctypes
import json
import os
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_ROSTER = os.path.join(HERE, "roster.json")

# The fraction of extracted cues that must find a partner in the release before the cue-level CER
# means anything. #229.
#
# A fraction rather than a count, which is `CLAUDE.md`'s rule and which matters here because the
# roster's tracks run from 106 cues to 2,442. The gap this sits in is not close: eight of the nine
# entries pair every cue but four, one, one and none, which is 0.5% at worst -- and King Kong
# leaves 529 of 1,704 unpaired, which is 31%. Anywhere between 1% and 30% would do; 5% is the
# round number in the middle of a gap sixty times wider than the threshold's own precision.
#
# What it catches is a sidecar cut from a *different release* of the film. #175 taught the bench to
# check a sidecar's convention -- SDH against SDH -- and King Kong passes that check: both sides are
# SDH. Both sides being SDH says nothing about both sides being the same cut, and a cue that pairs
# by start time onto a neighbour's text scores as a wall of insertions.
PAIRED_ENOUGH = 0.95

CER = re.compile(r"^\s+all\s+\S+\s+\S+\s+(\S+)%\s+\S+%\s+\S+\s+(\S+)%", re.M)
# The track-level row: one transcript against another, cue boundaries and timing ignored. #169
# built it for the census and #229 found the bench needed it too -- see `PAIRED_ENOUGH`.
TRACK_CER = re.compile(r"^\s+track\s+\S+\s+\S+\s+(\S+)%\s+\S+\s+\S+\s+(\S+)%", re.M)
PAIRING = re.compile(
    r"^\s+cues: (\d+) extracted, (\d+) in the release, (\d+) with no partner", re.M
)
REPORT = re.compile(
    r"(\d+) cues from (\d+) images .*?glyphs (\d+) matched / (\d+) unmatched / (\d+) ambiguous"
    r" \(([\d.]+)% read\); fit ([\d.]+)"
)
# The second line `--report` prints. Separate from REPORT because it is a separate question: one
# line says what the run read, the other what it cost, and nothing should have to parse both to
# learn either.
COST = re.compile(
    r"decode ([\d.]+)s; segment ([\d.]+)s; cluster ([\d.]+)s; read ([\d.]+)s; total ([\d.]+)s;"
    r" resident ([\d.]+) MiB images / ([\d.]+) MiB glyphs"
)
COMPARE = re.compile(
    r"cues improved : (\d+).*?cues worse\s+: (\d+).*?CER before\s+:\s*([\d.]+)%"
    r".*?CER after\s+:\s*([\d.]+)%",
    re.S,
)


def load_roster(path):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def media_path(roster, track):
    """The largest `.mkv` in the title's folder, which is the feature rather than an extra."""
    directory = os.path.join(roster["root"], track["folder"])
    files = [f for f in os.listdir(directory) if f.lower().endswith(".mkv")]
    if not files:
        raise SystemExit(f"no .mkv in {directory}")
    return max((os.path.join(directory, f) for f in files), key=os.path.getsize)


def sup_path(cache, track):
    return os.path.join(cache, f"{track['key']}.sup")


class Measured:
    """A finished command, with what it cost as well as what it said."""

    def __init__(self, proc, stdout, stderr, seconds, peak_bytes):
        self.returncode = proc.returncode
        self.stdout = stdout
        self.stderr = stderr
        self.seconds = seconds
        self.peak_bytes = peak_bytes


class _MemoryCounters(ctypes.Structure):
    _fields_ = [("cb", ctypes.c_uint32), ("PageFaultCount", ctypes.c_uint32),
                ("PeakWorkingSetSize", ctypes.c_size_t), ("WorkingSetSize", ctypes.c_size_t),
                ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPagedPoolUsage", ctypes.c_size_t),
                ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
                ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
                ("PagefileUsage", ctypes.c_size_t), ("PeakPagefileUsage", ctypes.c_size_t)]


def peak_working_set(proc):
    """Peak resident bytes of a finished child, or None where that cannot be measured.

    `None` rather than a zero or a guess. A cost table with a blank in it says the harness could
    not measure that platform; a zero would read as a process that used no memory, which is the
    invented-data failure `CLAUDE.md` forbids.

    Windows keeps the counters queryable after exit as long as a handle is open, and `Popen` holds
    one until it is collected -- so this must be called before the object goes out of scope.
    """
    if sys.platform != "win32":
        return None
    counters = _MemoryCounters()
    counters.cb = ctypes.sizeof(_MemoryCounters)
    ok = ctypes.windll.psapi.GetProcessMemoryInfo(
        int(proc._handle), ctypes.byref(counters), counters.cb)
    return counters.PeakWorkingSetSize if ok else None


def run(cmd, timeout=3600):
    """Run a command to completion, measuring wall clock and peak resident bytes.

    `Popen` rather than `subprocess.run` only because the memory counters need the handle, which
    `run` closes before returning.
    """
    started = time.perf_counter()
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, errors="replace")
    stdout, stderr = proc.communicate(timeout=timeout)
    seconds = time.perf_counter() - started
    return Measured(proc, stdout, stderr, seconds, peak_working_set(proc))


def do_dump(args, roster):
    os.makedirs(args.cache, exist_ok=True)
    for track in roster["tracks"]:
        out = sup_path(args.cache, track)
        if os.path.exists(out) and os.path.getsize(out) > 0:
            print(f"  {track['key']:15s} cached")
            continue
        # A `.sup` holds PGS and nothing else, so a VOBSUB track has nowhere to be dumped to and
        # `score` reads it from its container instead. Said rather than retried three times and
        # reported as a failure, which is what it did when #140 put the first one on the bench.
        if track.get("codec") == "vobsub":
            print(f"  {track['key']:15s} vobsub, read from the container")
            continue
        print(f"  {track['key']:15s} dumping...", flush=True)
        # Retried because a read over SMB fails about 1.7% of the time and succeeds next attempt.
        # #133 measured that; it is not a defect and it is not worth losing a 30-minute pass to.
        for attempt in range(3):
            result = run([args.xtask, "dump-sup", media_path(roster, track), out,
                          "--stream", str(track["stream"])])
            if os.path.exists(out) and os.path.getsize(out) > 0:
                print(f"  {track['key']:15s} {os.path.getsize(out) / 1048576:.1f} MB")
                break
            print(f"  {track['key']:15s} retry {attempt + 1}: "
                  f"{(result.stderr or '').strip()[:80]}", flush=True)
        else:
            print(f"  {track['key']:15s} FAILED")


def do_score(args, roster):
    os.makedirs(args.out_dir, exist_ok=True)
    results = {"reference": os.path.basename(args.reference), "extra": args.extra, "tracks": {}}

    for track in roster["tracks"]:
        key = track["key"]
        source = sup_path(args.cache, track)
        if not os.path.exists(source):
            source = media_path(roster, track)
        srt = os.path.join(args.out_dir, f"{key}.srt")

        cmd = [args.binary, "extract", source, "--reference", args.reference,
               "--format", "srt", "-o", srt, "--report"]
        # A `.sup` holds one track, so a stream selector would be wrong there and is only correct
        # against the container.
        if source.lower().endswith((".mkv", ".m2ts", ".mp4")):
            cmd += ["--stream", str(track["stream"])]
        cmd += args.extra

        result = run(cmd)
        entry = {"kind": track["kind"], "covers": track["covers"],
                 "seconds": round(result.seconds, 2)}
        if result.peak_bytes is not None:
            entry["peak_mib"] = round(result.peak_bytes / 1048576, 1)
        cost = COST.search(result.stderr or "")
        if cost:
            entry.update(
                decode_s=float(cost.group(1)),
                segment_s=float(cost.group(2)),
                cluster_s=float(cost.group(3)),
                read_s=float(cost.group(4)),
                image_mib=float(cost.group(6)),
                glyph_mib=float(cost.group(7)),
            )
        report = REPORT.search(result.stderr or "")
        if report:
            entry.update(
                cues=int(report.group(1)),
                matched=int(report.group(3)),
                unmatched=int(report.group(4)),
                ambiguous=int(report.group(5)),
                read=float(report.group(6)),
                fit=float(report.group(7)),
            )
        if result.returncode != 0:
            entry["error"] = (result.stderr or "").strip()[:200]
            results["tracks"][key] = entry
            print(f"  {key:15s} FAILED  {entry['error'][:60]}")
            continue

        if track["kind"] == "scored":
            release = os.path.join(roster["root"], track["folder"], track["sidecar"])
            scored = run([args.xtask, "srt-score", srt, release, "--align"])
            cue_level, track_level = CER.search(scored.stdout or ""), \
                TRACK_CER.search(scored.stdout or "")
            pairing = PAIRING.search(scored.stdout or "")
            # Recorded on every entry, not only the one that fails. The count was printed on every
            # run for as long as `srt-score` has existed and was in no results file, so nothing
            # could see that one track had been pairing a third of its cues onto the wrong text.
            if pairing:
                entry["cues_extracted"] = int(pairing.group(1))
                entry["cues_in_release"] = int(pairing.group(2))
                entry["cues_unpaired"] = int(pairing.group(3))
            paired = 1.0 - entry.get("cues_unpaired", 0) / max(entry.get("cues_extracted", 1), 1)
            entry["scored_at"] = "cue" if paired >= PAIRED_ENOUGH else "track"
            source = cue_level if entry["scored_at"] == "cue" else track_level
            if source:
                entry["cer"] = float(source.group(1))
                entry["wer"] = float(source.group(2))
            note = "" if entry["scored_at"] == "cue" else (
                f"  <- track-level; {entry.get('cues_unpaired', 0)} cues unpaired")
            print(f"  {key:15s} CER {entry.get('cer', float('nan')):5.1f}%  "
                  f"WER {entry.get('wer', float('nan')):5.1f}%  read {entry.get('read', 0):5.1f}%  "
                  f"fit {entry.get('fit', 0):4.1f}{note}")
        else:
            # No sidecar, so no accuracy claim. What this asserts is that the track came through:
            # cues out, glyphs read, no error. #133 put it here for the shape rather than the score.
            print(f"  {key:15s} smoke   {entry.get('cues', 0):5d} cues  "
                  f"read {entry.get('read', 0):5.1f}%  fit {entry.get('fit', 0):4.1f}")
        results["tracks"][key] = entry

    with open(args.out, "w", encoding="utf-8") as fh:
        json.dump(results, fh, indent=2)
    print(f"\n-> {args.out}")


def do_compare(args, roster):
    before = json.load(open(args.before, encoding="utf-8"))
    after = json.load(open(args.after, encoding="utf-8"))
    before_dir = os.path.splitext(args.before)[0]
    after_dir = os.path.splitext(args.after)[0]

    print(f"{'track':16s} {'CER before':>10s} {'CER after':>10s} {'delta':>7s} "
          f"{'better':>7s} {'worse':>7s}")
    regressions = 0
    for track in roster["tracks"]:
        key = track["key"]
        old, new_entry = before["tracks"].get(key, {}), after["tracks"].get(key, {})
        if track["kind"] != "scored":
            oc, nc = old.get("cues", 0), new_entry.get("cues", 0)
            flag = "" if oc == nc else "   <- CUE COUNT MOVED"
            print(f"{key:16s} {'smoke':>10s} {'':>10s} {'':>7s} "
                  f"{oc:>7d} {nc:>7d}{flag}")
            continue
        old_srt = os.path.join(before_dir, f"{key}.srt")
        new_srt = os.path.join(after_dir, f"{key}.srt")
        release = os.path.join(roster["root"], track["folder"], track["sidecar"])
        if not (os.path.exists(old_srt) and os.path.exists(new_srt)):
            print(f"{key:16s} {'missing srt':>10s}")
            continue
        # The positional is the OLD extraction and --compare is the new one. Reversing it reports
        # every improvement as a regression.
        result = run([args.xtask, "srt-score", old_srt, release, "--compare", new_srt])
        match = COMPARE.search(result.stdout or "")
        if not match:
            print(f"{key:16s} {'compare failed':>10s}")
            continue
        better, worse = int(match.group(1)), int(match.group(2))
        # CER comes from the results files rather than from `--compare`, which cannot take
        # `--align` and so scores against an unaligned sidecar. Mixing the two would print one
        # number here and a different one from `score` for the same extraction -- Airplane! reads
        # 41.9% aligned and 25.1% not. Only `better`/`worse` are taken from the comparison.
        cer_before = old.get("cer", float(match.group(3)))
        cer_after = new_entry.get("cer", float(match.group(4)))
        regressions += worse
        flag = "   <- REGRESSION" if worse > better else ""
        # An entry scored track-level says so on its own row, because the two columns beside it do
        # not change meaning and the CER column does. `better`/`worse` still come from a cue-level
        # comparison there and are still worth reading: a mispaired cue is mispaired identically in
        # both runs, so the pairing adds noise to that column rather than a direction.
        if "track" in (old.get("scored_at"), new_entry.get("scored_at")):
            flag += "   (CER track-level)"
        print(f"{key:16s} {cer_before:9.1f}% {cer_after:9.1f}% "
              f"{cer_before - cer_after:+7.1f} {better:>7d} {worse:>7d}{flag}")

    print(f"\n  cues made worse across the bench: {regressions}")
    print("  Read that column, not the CER. #110 gained character error on one disc while making")
    print("  232 cues worse on another, and #113 found it only because two more discs were scored.")

    # Cost second, and visibly second. #154 added it so that a refactor justified by cost has
    # something to be judged against -- but a change that is faster and reads worse has still made
    # things worse, so this table must never be the one read first.
    print(f"\n{'track':16s} {'sec before':>10s} {'sec after':>10s} {'delta':>7s} "
          f"{'MiB before':>10s} {'MiB after':>10s}")
    for track in roster["tracks"]:
        key = track["key"]
        old_entry, new_entry = before["tracks"].get(key, {}), after["tracks"].get(key, {})
        if "seconds" not in old_entry or "seconds" not in new_entry:
            # A results file written before #154 carries no cost at all, which is a measurement
            # that was never taken rather than a cost of zero.
            print(f"{key:16s} {'not measured':>10s}")
            continue
        was, now = old_entry["seconds"], new_entry["seconds"]
        old_peak, new_peak = old_entry.get("peak_mib"), new_entry.get("peak_mib")
        peak = (f"{old_peak:>10.1f} {new_peak:>10.1f}"
                if old_peak is not None and new_peak is not None
                else f"{'--':>10s} {'--':>10s}")
        print(f"{key:16s} {was:>10.2f} {now:>10.2f} {now - was:+7.2f} {peak}")


def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("mode", choices=["dump", "score", "compare"])
    ap.add_argument("before", nargs="?", help="compare: the earlier results file")
    ap.add_argument("after", nargs="?", help="compare: the later results file")
    ap.add_argument("--roster", default=DEFAULT_ROSTER)
    ap.add_argument("--cache", default="bench-cache", help="where the .sup dumps live")
    ap.add_argument("--reference", help="the .subtref to match against")
    ap.add_argument("--out", help="score: where to write the results file")
    # `normpath` because Windows `CreateProcess` refuses a *relative* executable spelled with
    # forward slashes -- `FileNotFoundError` before the first track dumps, on the platform this is
    # developed on. Applied to whatever the caller passes as well as to the defaults, so a path
    # copied out of the documentation works too.
    ap.add_argument("--binary", default="target/release/subtrackt.exe", type=os.path.normpath)
    ap.add_argument("--xtask", default="target/release/xtask.exe", type=os.path.normpath)
    # Anything argparse does not recognise is passed straight through to `extract`, so pricing a
    # flag is `run.py score ... --post-correct` rather than a wrapper that has to learn every flag
    # the CLI grows.
    args, extra = ap.parse_known_args()
    args.extra = extra

    roster = load_roster(args.roster)

    if args.mode == "dump":
        do_dump(args, roster)
    elif args.mode == "score":
        if not (args.reference and args.out):
            ap.error("score needs --reference and --out")
        # Sibling directory of the results file, so `compare` can find the SRTs from the JSON path.
        args.out_dir = os.path.splitext(args.out)[0]
        do_score(args, roster)
    else:
        if not (args.before and args.after):
            ap.error("compare needs two results files")
        do_compare(args, roster)


if __name__ == "__main__":
    main()
