#!/usr/bin/env python3
"""Score the #131 run and emit the tables for `docs/alternatives.md`.

Scoring runs on the host, outside every timed region, through the same `xtask srt-score` that
produced every accuracy figure this project has published. One instrument for every row is the whole
method: a competitor number produced by a second code path would have to be independently trusted,
and nothing here would be able to say whether a gap was the engines or the scorers.

Normalisation happens HERE, identically for every engine including subtrackt, and never inside
`srt-score`. Widening `strip_tags` to `{...}` would silently move every published figure, because
release sidecars carry `{\\an8}` too. Both files are kept -- `out.raw.srt` as the engine wrote it and
`out.srt` after normalisation -- and `normalised_chars_removed` is reported per engine so a reader
can see the normalisation was not load-bearing.
"""
import argparse
import collections
import json
import os
import re
import statistics
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
BENCH = os.path.join(ROOT, 'bench')
RESULTS = os.path.join(BENCH, 'results')
CORPUS = os.path.join(BENCH, 'corpus')
XTASK = os.path.join(ROOT, 'target', 'release', 'xtask.exe')
if not os.path.exists(XTASK):
    XTASK = os.path.join(ROOT, 'target', 'release', 'xtask')

sys.path.insert(0, HERE)
import engines as ENG  # noqa: E402

# ASS/SSA override blocks. Competitor output may carry `{\an8}`; so do release sidecars, which is
# exactly why this lives here and not in the scorer.
ASS_OVERRIDE = re.compile(r'\{[^}]*\}')

# Curly quotes, en and em dashes. A SENSITIVITY row, never the headline: folding these helps
# whichever engine emits the typographic form, so it is reported with its delta rather than baked in.
FOLD = {
    '‘': "'", '’': "'", '“': '"', '”': '"',
    '–': '-', '—': '-', '…': '...', ' ': ' ',
}


def normalise(text, fold=False):
    """Strip BOM, normalise CRLF, remove ASS overrides, drop cues empty after stripping."""
    removed = 0
    text = text.lstrip('﻿').replace('\r\n', '\n').replace('\r', '\n')
    stripped = ASS_OVERRIDE.sub('', text)
    removed += len(text) - len(stripped)
    text = stripped
    if fold:
        for src, dst in FOLD.items():
            text = text.replace(src, dst)

    blocks, out, index = text.split('\n\n'), [], 0
    for block in blocks:
        lines = [ln for ln in block.split('\n') if ln.strip()]
        if len(lines) < 2:
            continue
        # index, timing, then text
        timing = next((ln for ln in lines if '-->' in ln), None)
        if timing is None:
            continue
        body = lines[lines.index(timing) + 1:]
        if not any(ln.strip() for ln in body):
            continue
        index += 1
        out.append(str(index) + '\n' + timing + '\n' + '\n'.join(body))
    return '\n\n'.join(out) + '\n', removed


def count_unread_markers(text):
    """(4) of the issue comment: nOCR writes a literal `*` where it could not match.

    Counted rather than let to read as a zero -- and reported as a MARKER, not a count. A `*` inline
    in subtitle text cannot be gated on without parsing the output, and cannot be told apart from a
    `*` the disc actually displayed. subtrackt's own unread character is U+FFFD.
    """
    body = []
    for block in text.split('\n\n'):
        lines = block.split('\n')
        timing = next((i for i, ln in enumerate(lines) if '-->' in ln), None)
        if timing is not None:
            body.extend(lines[timing + 1:])
    joined = '\n'.join(body)
    return {
        'asterisk': joined.count('*'),
        'replacement': joined.count('�'),
    }


def load_corpus():
    with open(os.path.join(HERE, 'corpus.json'), encoding='utf-8') as fh:
        return json.load(fh)


def score(got_path, want_path):
    # `encoding` explicitly, because `text=True` alone decodes with the Windows ANSI codepage and
    # `srt-score` writes UTF-8: the fixture's own `Cafe`/`naive` cues carry accented characters, and
    # a cp1252 decode raises inside subprocess's reader thread, leaving stdout as None rather than
    # as an error anyone would recognise.
    proc = subprocess.run(
        [XTASK, 'srt-score', got_path, want_path, '--json', '--align'],
        capture_output=True, text=True, encoding='utf-8', errors='replace')
    if proc.returncode != 0:
        return None
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError:
        return None


def records():
    for name in sorted(os.listdir(RESULTS)):
        if not name.endswith('.json') or name in ('floor.json', 'cold-warm.json', 'scored.json'):
            continue
        with open(os.path.join(RESULTS, name), encoding='utf-8') as fh:
            yield name[:-5], json.load(fh)


def do_score(args):
    corpus = load_corpus()
    items = {i['key']: i for i in corpus['items']}
    root = corpus['root']
    out = []

    for name, rec in records():
        item = items[rec['item']]
        raw_path = os.path.join(RESULTS, name, 'out.raw.srt')
        entry = dict(rec)
        entry.pop('time_v', None)
        entry.pop('stderr_tail', None)
        entry.pop('stdout_tail', None)

        if not os.path.exists(raw_path):
            entry['scored'] = None
            entry['status'] = entry.get('status', 'no-output')
            out.append(entry)
            continue

        with open(raw_path, encoding='utf-8', errors='replace') as fh:
            raw = fh.read()
        entry['markers'] = count_unread_markers(raw)

        clean, removed = normalise(raw)
        entry['normalised_chars_removed'] = removed
        norm_path = os.path.join(RESULTS, name, 'out.srt')
        with open(norm_path, 'w', encoding='utf-8', newline='\n') as fh:
            fh.write(clean)

        folded, _ = normalise(raw, fold=True)
        fold_path = os.path.join(RESULTS, name, 'out.folded.srt')
        with open(fold_path, 'w', encoding='utf-8', newline='\n') as fh:
            fh.write(folded)

        # Which transcript this row is scored against, decided once in corpus.json and frozen.
        if item['kind'] != 'scored':
            entry['scored'] = None
            entry['status'] = 'not-scored:' + item['kind']
            out.append(entry)
            continue

        if item['key'] == 'fixture':
            want = os.path.join(CORPUS, 'truth', 'fixture.srt')
        else:
            want = os.path.join(root, item['folder'], item['sidecar'])

        entry['scored'] = score(norm_path, want)
        entry['scored_folded'] = score(fold_path, want)
        entry['sidecar'] = item.get('sidecar')
        out.append(entry)
        print('scored', name)

    with open(os.path.join(RESULTS, 'scored.json'), 'w', encoding='utf-8', newline='\n') as fh:
        json.dump(out, fh, indent=2)
    print('\n{} records -> scored.json'.format(len(out)))


def _median(values):
    clean = [v for v in values if v is not None]
    return statistics.median(clean) if clean else None


def _spread(values):
    clean = [v for v in values if v is not None]
    if len(clean) < 2:
        return 0.0
    return max(clean) - min(clean)


def load_scored():
    with open(os.path.join(RESULTS, 'scored.json'), encoding='utf-8') as fh:
        return json.load(fh)


def group(rows):
    """(engine, item) -> list of repeats."""
    out = collections.defaultdict(list)
    for r in rows:
        out[(r['engine'], r['item'])].append(r)
    return out


def do_tables(args):
    rows = load_scored()
    corpus = load_corpus()
    items = {i['key']: i for i in corpus['items']}
    grouped = group(rows)
    order = [e['id'] for e in ENG.all_engines()]

    def label(engine_id):
        return ENG.by_id(engine_id)['label']

    print('\n## Accuracy — cue level (`all`) and track level, one pinned sidecar per title\n')
    scored_items = [k for k, v in items.items() if v['kind'] == 'scored']
    header = '| engine | ' + ' | '.join(scored_items) + ' |'
    print(header)
    print('| :--- | ' + ' | '.join('---:' for _ in scored_items) + ' |')
    for engine_id in order:
        cells = []
        for key in scored_items:
            reps = grouped.get((engine_id, key), [])
            got = [r['scored'] for r in reps if r.get('scored')]
            if not got:
                cells.append('—')
                continue
            cue = _median([g['all']['cer'] for g in got])
            trk = _median([g['track']['cer'] for g in got])
            cells.append('{:.1f}% / {:.1f}%'.format(cue, trk))
        if any(c != '—' for c in cells):
            print('| {} | {}'.format(label(engine_id), ' | '.join(cells)) + ' |')

    print('\n## Cue accounting — extracted / release / unpaired (repeat 1)\n')
    print('| engine | item | extracted | release | unpaired |')
    print('| :--- | :--- | ---: | ---: | ---: |')
    for engine_id in order:
        for key in scored_items:
            reps = grouped.get((engine_id, key), [])
            got = [r['scored'] for r in reps if r.get('scored')]
            if not got:
                continue
            g = got[0]
            print('| {} | {} | {} | {} | {} |'.format(
                label(engine_id), key, g['cues_extracted'], g['cues_release'], g['cues_unpaired']))

    print('\n## Time — median wall, spread, CPU seconds, %CPU\n')
    print('| engine | item | wall s | spread s | cpu s | %cpu | rss MB | cgroup MB |')
    print('| :--- | :--- | ---: | ---: | ---: | ---: | ---: | ---: |')
    for engine_id in order:
        for key in items:
            reps = grouped.get((engine_id, key), [])
            if not reps:
                continue
            walls = [r.get('wall_s') for r in reps]
            cpus = [r.get('cpu_s') for r in reps]
            pcts = [r.get('pct_cpu') for r in reps]
            rss = [r.get('max_rss_kb') for r in reps]
            peaks = []
            for r in reps:
                p = str(r.get('cgroup_peak_bytes', '')).strip()
                if p.isdigit():
                    peaks.append(int(p))
            print('| {} | {} | {} | {} | {} | {} | {} | {} |'.format(
                label(engine_id), key,
                _fmt(_median(walls)), _fmt(_spread(walls)), _fmt(_median(cpus)),
                _fmt(_median(pcts), 0),
                _fmt((_median(rss) or 0) / 1024, 1),
                _fmt((_median(peaks) or 0) / 1048576, 1) if peaks else '—'))

    print('\n## Failures — attempted / completed / failed\n')
    tally = collections.defaultdict(lambda: [0, 0, 0])
    for r in rows:
        t = tally[r['engine']]
        t[0] += 1
        if r.get('rc') == 0 and r.get('out_bytes'):
            t[1] += 1
        else:
            t[2] += 1
    print('| engine | attempted | completed | failed |')
    print('| :--- | ---: | ---: | ---: |')
    for engine_id in order:
        if engine_id in tally:
            a, c, f = tally[engine_id]
            print('| {} | {} | {} | {} |'.format(label(engine_id), a, c, f))

    print('\n## Machine-readable "could not read" — the column the argument rests on\n')
    print('| engine | item | U+FFFD (gateable) | `*` (inline marker) |')
    print('| :--- | :--- | ---: | ---: |')
    for engine_id in order:
        for key in items:
            reps = [r for r in grouped.get((engine_id, key), []) if r.get('markers')]
            if not reps:
                continue
            m = reps[0]['markers']
            if m['asterisk'] or m['replacement']:
                print('| {} | {} | {} | {} |'.format(
                    label(engine_id), key, m['replacement'], m['asterisk']))

    print('\n## Determinism — SHA-256 of output across repeats\n')
    print('| engine | item | distinct outputs |')
    print('| :--- | :--- | ---: |')
    for engine_id in order:
        for key in items:
            reps = grouped.get((engine_id, key), [])
            digests = {r.get('out_sha256') for r in reps if r.get('out_sha256')}
            if len(digests) > 1:
                print('| {} | {} | **{}** |'.format(label(engine_id), key, len(digests)))

    print('\n## Alignment agreement — (scale, offset_ms) must be identical across engines\n')
    for key in scored_items:
        seen = collections.defaultdict(list)
        for engine_id in order:
            for r in grouped.get((engine_id, key), []):
                s = r.get('scored')
                if s and s.get('alignment'):
                    a = s['alignment']
                    seen[(round(a['scale'], 6), a['offset_ms'])].append(engine_id)
        print('- **{}**: {}'.format(key, dict(
            (str(k), sorted(set(v))) for k, v in seen.items()) if seen else 'no alignment reported'))


def _best(grouped, engine_ids, key, field):
    """The best (lowest) median CER among a set of engines on one item."""
    best = None
    for engine_id in engine_ids:
        got = [r['scored'] for r in grouped.get((engine_id, key), []) if r.get('scored')]
        if not got:
            continue
        value = _median([g[field]['cer'] for g in got])
        if value is not None and (best is None or value < best[1]):
            best = (engine_id, value)
    return best


def do_predictions(args):
    """Score the predictions #131 committed to before anything ran.

    Several of them were deliberately predictions that we lose, and a prediction whose premise was
    wrong is worth more visible than tidied -- so the premise failures are reported as such rather
    than quietly restated.
    """
    rows = load_scored()
    corpus = load_corpus()
    items = {i['key']: i for i in corpus['items']}
    grouped = group(rows)

    subtrackt = [e['id'] for e in ENG.ENGINES if e['tool'] == 'subtrackt']
    tesseract = [e['id'] for e in ENG.ENGINES if e['family'] == 'tesseract']
    competitors = [e['id'] for e in ENG.ENGINES if e['tool'] != 'subtrackt']
    scored_items = [k for k, v in items.items() if v['kind'] == 'scored']

    def walls(engine_id, key):
        return [r.get('wall_s') for r in grouped.get((engine_id, key), [])]

    def cpus(engine_id, key):
        return [r.get('cpu_s') for r in grouped.get((engine_id, key), [])]

    def rss_mb(engine_id):
        out = []
        for key in items:
            for r in grouped.get((engine_id, key), []):
                if r.get('max_rss_kb'):
                    out.append(r['max_rss_kb'] / 1024)
        return out

    print('### Predictions, scored\n')

    # 1. Speed.
    print('**1. subtrackt at least 20x faster per cue than the fastest Tesseract tool, and the '
          'ratio larger in CPU-seconds than in wall clock.**\n')
    print('| item | subtrackt-arial wall | fastest Tesseract wall | wall ratio | cpu ratio |')
    print('| :--- | ---: | ---: | ---: | ---: |')
    for key in items:
        mine = _median(walls('subtrackt-arial', key))
        mine_cpu = _median(cpus('subtrackt-arial', key))
        best_wall, best_id = None, None
        for e in tesseract:
            v = _median(walls(e, key))
            if v is not None and (best_wall is None or v < best_wall):
                best_wall, best_id = v, e
        if mine and best_wall:
            best_cpu = _median(cpus(best_id, key))
            wall_ratio = best_wall / mine
            cpu_ratio = (best_cpu / mine_cpu) if (best_cpu and mine_cpu) else None
            print('| {} | {:.2f} | {:.2f} ({}) | **{:.0f}x** | {} |'.format(
                key, mine, best_wall, best_id, wall_ratio,
                '{:.0f}x'.format(cpu_ratio) if cpu_ratio else '-'))

    # 2. Memory.
    print('\n**2. subtrackt peak RSS under 60 MB; every .NET tool over 200 MB; every Tesseract '
          'tool over 300 MB; ratio at least 5x.**\n')
    print('| engine | peak RSS MB (max over corpus) |')
    print('| :--- | ---: |')
    for e in ENG.ENGINES:
        values = rss_mb(e['id'])
        if values:
            print('| {} | {:.0f} |'.format(e['label'], max(values)))

    # 3-5. Accuracy.
    print('\n**3. We lose on the fixture: at least one Tesseract tool under 3% CER.**')
    print('**4. We win on the discs when fitted, but the gap is under 3 points on at least one '
          'title.**')
    print('**5. We lose out of the box: subtrackt-liberation loses to at least one Tesseract tool '
          'on at least one disc.**\n')
    print('| item | subtrackt-fitted | subtrackt-arial | subtrackt-liberation | best Tesseract | '
          'best competitor | sidecar spread |')
    print('| :--- | ---: | ---: | ---: | ---: | ---: | ---: |')
    for key in scored_items:
        cells = []
        for e in ['subtrackt-fitted', 'subtrackt-arial', 'subtrackt-liberation']:
            got = [r['scored'] for r in grouped.get((e, key), []) if r.get('scored')]
            cells.append(_median([g['track']['cer'] for g in got]) if got else None)
        bt = _best(grouped, tesseract, key, 'track')
        bc = _best(grouped, competitors, key, 'track')
        spread = items[key].get('selector_spread_track_points')
        print('| {} | {} | {} | {} | {} | {} | {} |'.format(
            key,
            _fmt(cells[0], 1), _fmt(cells[1], 1), _fmt(cells[2], 1),
            '{:.1f} ({})'.format(bt[1], bt[0]) if bt else '-',
            '{:.1f} ({})'.format(bc[1], bc[0]) if bc else '-',
            spread if spread is not None else '-'))
    print('\nTrack-level CER, median of three repeats. **A gap smaller than that title\'s sidecar '
          'spread is not a result** -- the spread is the instrument\'s own uncertainty.')

    # 9. The one the project would be most damaged by losing.
    print('\n**9. Nobody else tells you they failed: machine-readable "could not read" is zero '
          'for every competitor on every item.**\n')
    print('| engine | U+FFFD, gateable | `*`, inline marker |')
    print('| :--- | ---: | ---: |')
    for e in ENG.ENGINES:
        total_r = total_a = 0
        for key in items:
            for r in grouped.get((e['id'], key), []):
                m = r.get('markers') or {}
                total_r += m.get('replacement', 0)
                total_a += m.get('asterisk', 0)
        print('| {} | {} | {} |'.format(e['label'], total_r, total_a))

    # 10. Footprint.
    print('\n**10. Every competitor\'s install tree over 100 MB; subtrackt plus a set under 2 MB.**\n')
    print('Install-tree sizes are baked into the image at build time; see '
          '`/opt/manifest/sizes.txt`.\n')

    # 8. Cue counts.
    print('**8. Cue counts agree to within 1% across engines.**\n')
    print('| item | modal cues | engines deviating by more than 1% |')
    print('| :--- | ---: | :--- |')
    for key in scored_items:
        counts = {}
        for e in ENG.ENGINES:
            got = [r['scored'] for r in grouped.get((e['id'], key), []) if r.get('scored')]
            if got:
                counts[e['id']] = got[0]['cues_extracted']
        if not counts:
            continue
        modal = collections.Counter(counts.values()).most_common(1)[0][0]
        off = ['{} ({})'.format(k, v) for k, v in counts.items()
               if abs(v - modal) > modal * 0.01]
        print('| {} | {} | {} |'.format(key, modal, ', '.join(off) if off else 'none'))


def _fmt(value, places=2):
    if value is None:
        return '—'
    return '{:.{}f}'.format(value, places)


def main():
    # The tables carry em dashes and accented characters; a Windows console defaults to cp1252 and
    # replaces them. Reconfigure rather than strip, so the markdown that lands in the doc is the
    # markdown that was generated.
    try:
        sys.stdout.reconfigure(encoding='utf-8')
    except (AttributeError, OSError):
        pass
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest='cmd', required=True)
    sub.add_parser('score')
    sub.add_parser('tables')
    sub.add_parser('predictions')
    args = ap.parse_args()
    if args.cmd == 'score':
        do_score(args)
    elif args.cmd == 'predictions':
        do_predictions(args)
    else:
        do_tables(args)


if __name__ == '__main__':
    main()
