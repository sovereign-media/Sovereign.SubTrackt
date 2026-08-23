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
"""

import argparse
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_ROSTER = os.path.join(HERE, "roster.json")

CER = re.compile(r"^\s+all\s+\S+\s+\S+\s+(\S+)%\s+\S+%\s+\S+\s+(\S+)%", re.M)
REPORT = re.compile(
    r"(\d+) cues from (\d+) images .*?glyphs (\d+) matched / (\d+) unmatched / (\d+) ambiguous"
    r" \(([\d.]+)% read\); fit ([\d.]+)"
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


def run(cmd, timeout=3600):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, errors="replace")


def do_dump(args, roster):
    os.makedirs(args.cache, exist_ok=True)
    for track in roster["tracks"]:
        out = sup_path(args.cache, track)
        if os.path.exists(out) and os.path.getsize(out) > 0:
            print(f"  {track['key']:15s} cached")
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
        entry = {"kind": track["kind"], "covers": track["covers"]}
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
            match = CER.search(scored.stdout or "")
            if match:
                entry["cer"] = float(match.group(1))
                entry["wer"] = float(match.group(2))
            print(f"  {key:15s} CER {entry.get('cer', float('nan')):5.1f}%  "
                  f"WER {entry.get('wer', float('nan')):5.1f}%  read {entry.get('read', 0):5.1f}%  "
                  f"fit {entry.get('fit', 0):4.1f}")
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
        print(f"{key:16s} {cer_before:9.1f}% {cer_after:9.1f}% "
              f"{cer_before - cer_after:+7.1f} {better:>7d} {worse:>7d}{flag}")

    print(f"\n  cues made worse across the bench: {regressions}")
    print("  Read that column, not the CER. #110 gained character error on one disc while making")
    print("  232 cues worse on another, and #113 found it only because two more discs were scored.")


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
    ap.add_argument("--binary", default="target/release/subtrackt.exe")
    ap.add_argument("--xtask", default="target/release/xtask.exe")
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
