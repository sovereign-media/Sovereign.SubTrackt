#!/usr/bin/env python3
"""Aggregate the library sweep into the two tables."""
import collections
import glob
import json
import os
import re
import statistics
import sys

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

RES = sys.argv[1] if len(sys.argv) > 1 else "results"
SP = os.path.dirname(os.path.abspath(RES)) or "."

SDH = re.compile(r"sdh|hearing|[.-]cc[.-]", re.I)


def load():
    out = []
    for f in glob.glob(os.path.join(RES, "*.json")):
        try:
            out.append(json.load(open(f, encoding="utf-8")))
        except (json.JSONDecodeError, OSError):
            pass
    return out


def best(rec):
    """The sidecar that agrees most with the extraction.

    Which release a rip's sidecar came from is unknown, and an SDH transcript scored against a
    dialogue-only track differs for reasons the matcher has no part in. Taking the best-agreeing
    sidecar removes that confound; how much it moved is reported separately.
    """
    scores = rec.get("scores") or {}
    ok = [(n, s) for n, s in scores.items() if s.get("track", {}).get("chars", 0) > 0]
    if not ok:
        return None, None
    return min(ok, key=lambda kv: kv[1]["track"]["cer"])


def pct(xs, p):
    if not xs:
        return float("nan")
    xs = sorted(xs)
    k = (len(xs) - 1) * p / 100
    lo, hi = int(k), min(int(k) + 1, len(xs) - 1)
    return xs[lo] + (xs[hi] - xs[lo]) * (k - lo)


def rate(num, den):
    return 100.0 * num / den if den else float("nan")


def pooled(group, band="track"):
    C = sum(r["_best"][band]["chars"] for r in group)
    E = sum(r["_best"][band]["char_errors"] for r in group)
    W = sum(r["_best"][band]["words"] for r in group)
    WE = sum(r["_best"][band]["word_errors"] for r in group)
    return C, E, W, WE


def chars_table(group, label, floor=1000, top=45):
    seen = collections.Counter()
    wrong = collections.Counter()
    conf = collections.defaultdict(collections.Counter)
    for r in group:
        for row in r["_best"].get("characters", []):
            seen[row["char"]] += row["seen"]
            wrong[row["char"]] += row["wrong"]
        for c in r["_best"].get("confusions", []):
            conf[c["from"]][c["to"]] += c["upright"] + c["italic"]
    print()
    print("== per-character: %s (%d titles) ==" % (label, len(group)))
    rows = [(c, seen[c], wrong[c]) for c in seen if seen[c] >= floor]
    rows.sort(key=lambda t: -t[2] / t[1])
    print("  %-6s %10s %9s %8s   most-common misread" % ("char", "seen", "wrong", "rate"))
    for c, sn, w in rows[:top]:
        misread = ", ".join("%s %d" % (k, v) for k, v in conf[c].most_common(3))
        print("  %-6r %10d %9d %7.2f%%   %s" % (c, sn, w, rate(w, sn), misread))
    return {"seen": dict(seen), "wrong": dict(wrong),
            "conf": {k: dict(v) for k, v in conf.items()}}


def main():
    recs = load()
    ok, failed = [], []
    for r in recs:
        n, s = best(r)
        if r.get("error") or s is None:
            failed.append(r)
            continue
        r["_best_name"], r["_best"] = n, s
        ok.append(r)

    print("attempted        %d" % len(recs))
    print("scored           %d" % len(ok))
    print("failed           %d" % len(failed))
    reasons = collections.Counter((f.get("error") or "no score").split(":")[0] for f in failed)
    for k, v in reasons.most_common(10):
        print("    %5d  %s" % (v, k[:70]))

    print()
    print("== pooled (micro-average over every scored character) ==")
    print("  track     = one transcript per title; cue boundaries and timing ignored.")
    print("  cue-level = paired by start time; splits by style, but charges cue-boundary")
    print("              differences between the two releases as if they were misreads.")
    for band in ("track", "all", "upright", "italic"):
        C, E, W, WE = pooled(ok, band)
        print("  %-8s chars %10d CER %6.2f%%   words %9d WER %6.2f%%"
              % (band, C, rate(E, C), W, rate(WE, W)))

    cers = [r["_best"]["track"]["cer"] for r in ok]
    wers = [r["_best"]["track"]["wer"] for r in ok]
    print()
    print("== per-title distribution ==")
    for p in (5, 10, 25, 50, 75, 90, 95):
        print("  p%-3d CER %7.2f%%   WER %7.2f%%" % (p, pct(cers, p), pct(wers, p)))
    print("  mean CER %.2f%%   mean WER %.2f%%" % (statistics.fmean(cers), statistics.fmean(wers)))
    for t in (1, 2, 5, 10, 20):
        n = sum(1 for c in cers if c < t)
        print("  titles under %2d%% CER: %5d  (%.1f%%)" % (t, n, rate(n, len(cers))))

    secs = [r["extract_seconds"] for r in ok if r.get("extract_seconds")]
    gb = [(r.get("bytes") or 0) / 1e9 for r in ok if r.get("bytes")]
    thr = [(r["bytes"] or 0) / 1e6 / r["extract_seconds"]
           for r in ok if r.get("bytes") and r.get("extract_seconds")]
    cps = [r["_best"]["all"]["cues"] / r["extract_seconds"] for r in ok if r.get("extract_seconds")]
    print()
    print("== runtime (extraction only; 4 titles in parallel over SMB) ==")
    print("  mean %.1fs   median %.1fs   p90 %.1fs   max %.1fs   total %.2f core-hours"
          % (statistics.fmean(secs), statistics.median(secs), pct(secs, 90), max(secs),
             sum(secs) / 3600))
    print("  mean size %.2f GB   median %.2f GB" % (statistics.fmean(gb), statistics.median(gb)))
    print("  mean throughput %.0f MB/s   mean %.0f cues/s"
          % (statistics.fmean(thr), statistics.fmean(cps)))

    # Which *kind* of sidecar wins is not fixed: a disc whose own track is SDH is matched best by an
    # SDH sidecar, and one carrying dialogue only by a dialogue-only sidecar. So the useful split is
    # not SDH versus not -- it is whether the folder offered a choice at all. A title with one
    # candidate is scored against whatever it happened to ship with, matched or not.
    print()
    print("== was there a choice of sidecar? ==")
    def cands(r):
        return [s["track"]["cer"] for s in r["scores"].values() if s.get("track", {}).get("chars", 0)]
    one = [r for r in ok if len(cands(r)) == 1]
    many = [r for r in ok if len(cands(r)) > 1]
    for label, g in (("one candidate only", one), ("two or more, best taken", many)):
        if not g:
            continue
        C, E, W, WE = pooled(g)
        cs = [x["_best"]["track"]["cer"] for x in g]
        print("  %-24s titles %4d  CER %6.2f%%  WER %6.2f%%  median %6.2f%%  p90 %6.2f%%"
              % (label, len(g), rate(E, C), rate(WE, W), statistics.median(cs), pct(cs, 90)))
    spreads = [max(cands(r)) - min(cands(r)) for r in many]
    if spreads:
        print("  spread between candidates on the same title: median %.2f points, p90 %.2f, max %.2f"
              % (statistics.median(spreads), pct(spreads, 90), max(spreads)))
        print("  -- the size of this spread is how much of a title's score is the sidecar rather")
        print("     than the pipeline: the same extraction, scored against two transcripts.")

    print()
    print("== which kind of sidecar won ==")
    for label, g in (("SDH", [r for r in ok if SDH.search(r["_best_name"])]),
                     ("dialogue-only", [r for r in ok if not SDH.search(r["_best_name"])])):
        if g:
            print("  %-14s %4d titles" % (label, len(g)))

    # The per-character census is built from the cue-level alignment, so it inherits that
    # alignment's faults: where the sidecar transcribes something different, the aligner pairs
    # characters that were never meant to correspond and the census records the difference as a
    # confusion. On a title the sidecar broadly agrees with, that noise is small and what is left
    # is the matcher's own failures. This is a conditional question -- "among titles where the
    # transcript corroborates, which characters still fail?" -- and it is the only form of the
    # question the instrument can actually answer.
    corroborated = [r for r in ok if r["_best"]["track"]["cer"] < 5.0]
    out = {"all": chars_table(ok, "every scored title"),
           "corroborated": chars_table(corroborated,
                                       "titles the sidecar corroborates (track CER < 5%)")}
    json.dump(out, open(os.path.join(SP, "pooled_chars.json"), "w", encoding="utf-8"))

    print()
    print("== by decade ==")
    by = collections.defaultdict(list)
    for r in ok:
        try:
            by[int(r["year"]) // 10 * 10].append(r)
        except (ValueError, TypeError):
            pass
    for d in sorted(by):
        g = by[d]
        C, E, W, WE = pooled(g)
        med = statistics.median([x["_best"]["track"]["cer"] for x in g])
        print("  %ss  titles %4d  CER %6.2f%%  WER %6.2f%%  median-title CER %6.2f%%"
              % (d, len(g), rate(E, C), rate(WE, W), med))

    print()
    print("== by codec ==")
    byc = collections.defaultdict(list)
    for r in ok:
        byc[r["codecs"]].append(r)
    for k, g in sorted(byc.items()):
        C, E, W, WE = pooled(g)
        med = statistics.median([x["_best"]["track"]["cer"] for x in g])
        print("  %-34s titles %4d  CER %6.2f%%  WER %6.2f%%  median %6.2f%%"
              % (k, len(g), rate(E, C), rate(WE, W), med))

    print()
    print("== glyph coverage reported by the extractor ==")
    read = [r["glyphs"]["read_pct"] for r in ok if r.get("glyphs")]
    fit = [r["glyphs"]["fit"] for r in ok if r.get("glyphs")]
    if read:
        print("  read%%: median %.2f  p10 %.2f  min %.2f" % (statistics.median(read), pct(read, 10), min(read)))
        print("  fit:   median %.2f  p90 %.2f  max %.2f" % (statistics.median(fit), pct(fit, 90), max(fit)))

    # A bad score has two very different causes and they need telling apart. If the reference set
    # does not fit the title's typeface, the matcher is guessing and says so: mean match distance
    # rises. If the set fits and the glyphs were read confidently but the score is still bad, the
    # extraction and the sidecar are transcribing different things -- which is a fact about the two
    # releases, not about the pipeline.
    print()
    print("== is a bad score a bad read, or a different transcript? ==")
    g = [r for r in ok if r.get("glyphs")]
    print("  %-18s %6s %8s %8s %9s" % ("fit (match dist)", "titles", "med CER", "med WER", "med read%"))
    for lo, hi in ((0, 12), (12, 14), (14, 16), (16, 20), (20, 999)):
        b = [r for r in g if lo <= r["glyphs"]["fit"] < hi]
        if not b:
            continue
        print("  %-18s %6d %7.2f%% %7.2f%% %8.2f%%"
              % ("%g-%g" % (lo, hi), len(b),
                 statistics.median([x["_best"]["track"]["cer"] for x in b]),
                 statistics.median([x["_best"]["track"]["wer"] for x in b]),
                 statistics.median([x["glyphs"]["read_pct"] for x in b])))

    confident_bad = [r for r in g
                     if r["glyphs"]["read_pct"] >= 99.0 and r["glyphs"]["fit"] < 14
                     and r["_best"]["track"]["cer"] > 15]
    print("  titles read confidently (>=99%% of glyphs, fit <14) yet scoring over 15%% CER: %d of %d"
          % (len(confident_bad), len(g)))
    for r in sorted(confident_bad, key=lambda x: -x["_best"]["track"]["cer"])[:12]:
        print("     %6.2f%% CER  fit %5.2f  read %5.1f%%  %s  [%s]"
              % (r["_best"]["track"]["cer"], r["glyphs"]["fit"], r["glyphs"]["read_pct"],
                 r["folder"][:42], "SDH" if SDH.search(r["_best_name"]) else "dialogue"))


main()
