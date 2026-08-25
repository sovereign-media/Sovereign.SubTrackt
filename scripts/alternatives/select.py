#!/usr/bin/env python3
"""Turn a library sweep into corpus entries, applying the roster's own sidecar rule (#209).

`corpus.json` says the sidecar is chosen ONCE, by one engine, before any other engine runs, and
then frozen. For six items that was done by hand. For twenty it is done here, from the results of
`scripts/accuracy/sweep.py` -- which is the same selector (`subtrackt-arial` against a single Arial
set) and already scores every English sidecar in the folder rather than one.

**Choosing the best-agreeing sidecar is not enough, and #175 is why.** A Fish Called Wanda was
scored for months against a sidecar carrying none of its 85 bracketed sound cues; more than half its
measured error was the missing cues, and it reads 1.7% rather than 4.2% against the right one.
Airplane! is worse -- the disc renders sound cues as brackets and its own SDH sidecar renders them
as musical notes, so *neither* candidate matches and the title cannot be scored at all. So the
best-agreeing candidate is then put through a shape check, and a title whose best candidate fails it
is dropped from the draw rather than scored. A track that puts false entries in the column the bench
tells you to read poisons the one number you are told to read.

Both thresholds are fractions of something measured rather than absolute counts, per the house rule:
the same title ships at several resolutions and with sidecars of wildly different lengths.

**The shape check is not a filter on the outcome, and that distinction is the whole argument.** It
reads structure -- how many lines open with a bracket, how many cues paired -- and both survive a
garbled extraction, because brackets are structural and timings align even when the typeface does
not fit. So a title whose text the selector reads badly is *kept*: The Negotiator passes the shape
check at 25.5% CER and goes into the corpus, which is what stops this from being the circular
selection `docs/library-accuracy.md` warns about. What gets dropped is a sidecar transcribing a
different thing, never a title an engine finds hard.

Every rejection is written out beside the picks. A draw that silently walked past half the sample
would read as "twenty titles from the library" when it is not.

Usage:
    select.py --sweep sweep-results/ --count 20 --out picked.json
"""
import argparse
import hashlib
import json
import os
import re

# A line the disc renders as a sound cue rather than as speech. Brackets, parentheses and the
# musical note are the three conventions in this library; `docs/vobsub.md` has the survey.
SOUND_CUE = re.compile(r"^\s*[\[(♪♫]")

# A cue the extraction paired with nothing in the sidecar. Some unpaired cues are normal -- releases
# differ by a few lines -- but a tenth of the track means the two are transcribing different things.
MAX_UNPAIRED_FRACTION = 0.10

# How far apart the two sides' sound-cue counts may be before they are different conventions. Wanda
# read 85 against 0; Airplane! read 72 brackets against 197 notes. Both are far outside this.
MIN_SOUND_CUE_AGREEMENT = 0.50

# Below this many sound cues on either side, the track is dialogue-only and the ratio above is noise
# rather than evidence -- a track with three brackets says nothing about convention.
SOUND_CUE_FLOOR = 20


def sound_cues(text):
    return sum(1 for line in text.splitlines() if SOUND_CUE.match(line))


def read(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""


def shape(extraction, sidecar_text, scored):
    """Does this sidecar transcribe the same thing the track does?

    Returns (ok, reason). The reason is kept even when it passes, because the corpus records why a
    title is scored as well as why one is not.
    """
    extracted = scored.get("cues_extracted") or 0
    unpaired = scored.get("cues_unpaired") or 0
    if extracted and unpaired / extracted > MAX_UNPAIRED_FRACTION:
        return False, "unpaired {:.0%} of {} extracted cues".format(unpaired / extracted, extracted)

    mine, theirs = sound_cues(extraction), sound_cues(sidecar_text)
    if max(mine, theirs) >= SOUND_CUE_FLOOR:
        agreement = min(mine, theirs) / max(mine, theirs)
        if agreement < MIN_SOUND_CUE_AGREEMENT:
            return False, "sound cues {} in the track against {} in the sidecar".format(mine, theirs)
        return True, "sound cues {} against {}, {} unpaired".format(mine, theirs, unpaired)
    return True, "dialogue-only, {} unpaired of {}".format(unpaired, extracted)


def candidates(record, folder):
    """Every scored sidecar for one title, best-agreeing first."""
    out = []
    for name, s in (record.get("scores") or {}).items():
        out.append({
            "file": name,
            "selector_cue_cer_pct": round(s["all"]["cer"], 1),
            "selector_track_cer_pct": round(s["track"]["cer"], 1),
            "_scored": s,
            "_path": os.path.join(folder, name),
        })
    return sorted(out, key=lambda c: c["selector_track_cer_pct"])


def sha256_file(path):
    h = hashlib.sha256()
    try:
        with open(path, "rb") as fh:
            for block in iter(lambda: fh.read(1 << 20), b""):
                h.update(block)
    except OSError:
        return None
    return h.hexdigest()


def key_for(folder):
    """A corpus key: short, lowercase, and stable for the same folder."""
    words = re.findall(r"[A-Za-z0-9]+", folder.split("(")[0])
    return ("".join(words).lower() or "title")[:16]


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--sweep", required=True, help="directory of sweep per-title results")
    ap.add_argument("--count", type=int, default=20, help="titles to pick")
    ap.add_argument("--out", required=True, help="where to write the picked corpus entries")
    ap.add_argument("--exclude", default="", help="comma-separated folders already in the corpus")
    args = ap.parse_args()

    exclude = {f for f in args.exclude.split(",") if f}

    records = []
    for name in sorted(os.listdir(args.sweep)):
        if not name.endswith(".json"):
            continue
        with open(os.path.join(args.sweep, name), encoding="utf-8") as fh:
            records.append(json.load(fh))

    # Same ordering `sample.py` used to draw the sample: a hash of the folder name, which is
    # independent of year, size, codec and of anything the pipeline does. Drawing the corpus by the
    # same rule means the choice is not made on the outcome being measured.
    records.sort(key=lambda r: hashlib.sha256(r["folder"].encode("utf-8")).hexdigest())

    picked, rejected, skipped = [], [], []
    for rec in records:
        if len(picked) >= args.count:
            break
        if rec["folder"] in exclude:
            continue
        # Every engine in this comparison reads the same flat `.sup`, which holds PGS and nothing
        # else -- so a VOBSUB title has nowhere to be staged to. `scripts/bench/run.py` hit the same
        # wall in #140 and reads those from their container instead; that is not available here,
        # because feeding one engine an `.mkv` and the others a `.sup` is the demux difference the
        # whole corpus exists to exclude. The other codec is measured on the bench, not here.
        if "hdmv_pgs" not in (rec.get("codecs") or ""):
            skipped.append((rec["folder"], "not PGS: " + (rec.get("codecs") or "unknown")))
            continue
        if not rec.get("scores"):
            skipped.append((rec["folder"], rec.get("error") or "no sidecar scored"))
            continue
        folder_path = os.path.dirname(rec["mkv"])
        cands = candidates(rec, folder_path)
        best = cands[0]
        extraction = read(rec.get("srt") or "")
        ok, reason = shape(extraction, read(best["_path"]), best["_scored"])

        spread = (cands[-1]["selector_track_cer_pct"] - cands[0]["selector_track_cer_pct"]
                  if len(cands) > 1 else 0.0)
        stream = (rec.get("track") or {}).get("index", 0)
        entry = {
            "key": key_for(rec["folder"]),
            "kind": "scored",
            "truth": "sidecar",
            "sup": key_for(rec["folder"]) + ".sup",
            "folder": rec["folder"],
            "stream": stream,
            "sidecar": best["file"],
            "note": "Drawn by #209 from the library sweep. Stream {}, {}, {} cues extracted. "
                    "Shape check: {}.".format(
                        stream, rec["codecs"], best["_scored"].get("cues_extracted"), reason),
            "sidecar_candidates": [
                {"file": c["file"], "selector_cue_cer_pct": c["selector_cue_cer_pct"],
                 "selector_track_cer_pct": c["selector_track_cer_pct"],
                 "chosen": c is best}
                for c in cands],
            "selector_spread_track_points": round(spread, 1),
        }
        if not ok:
            # Recorded rather than dropped. `--count` counts scored titles, so a rejected one does
            # not consume a slot -- but a draw that quietly walked past half the sample would read
            # as "twenty titles from the library" when it is not, so the list is published.
            rejected.append({
                "folder": rec["folder"],
                "reason": reason,
                "best_candidate": best["file"],
                "best_selector_track_cer_pct": best["selector_track_cer_pct"],
            })
            continue

        entry["sidecar_sha256"] = sha256_file(best["_path"])
        entry["sidecar_bytes"] = (os.path.getsize(best["_path"])
                                  if os.path.exists(best["_path"]) else None)
        picked.append(entry)

    out = {"picked": picked, "rejected": rejected, "unscoreable": skipped,
           "records_walked": len(records)}
    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(out, fh, indent=2)

    print("walked %d records -> %d scored, %d rejected on shape, %d with no sidecar scored"
          % (len(records), len(picked), len(rejected), len(skipped)))
    for e in picked:
        print("  %-18s %5s%%  %s" % (
            e["key"], e["sidecar_candidates"][0]["selector_track_cer_pct"],
            e["note"].split("Shape check: ")[-1]))
    if rejected:
        print("\nrejected on shape:")
        for r in rejected:
            print("  %-42s %s" % (r["folder"][:42], r["reason"]))
    if skipped:
        print("\nno sidecar scored:")
        for folder, why in skipped:
            print("  %-42s %s" % (folder[:42], why))


main()
