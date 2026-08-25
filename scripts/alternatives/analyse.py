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


def _pct(values, percentile):
    """Nearest-rank, on an already-sorted list. Twenty titles is too few to interpolate."""
    if not values:
        return None
    return values[min(len(values) - 1, int(round(percentile / 100 * len(values) + 0.5)) - 1)]


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

    scored_items = [k for k, v in items.items() if v['kind'] == 'scored']
    cost_items = [k for k, v in items.items() if v.get('cost')]

    def cer(engine_id, key, field='track'):
        got = [r['scored'] for r in grouped.get((engine_id, key), []) if r.get('scored')]
        return _median([g[field]['cer'] for g in got]) if got else None

    # The headline. A twenty-column grid is not a table anyone reads, and the reason for widening
    # the corpus was never to publish twenty numbers per engine -- it was to turn "no competitor is
    # good on all three" from an anecdote into a distribution. The grid survives as an appendix.
    main_order = [e['id'] for e in ENG.ENGINES]
    sens_order = [e['id'] for e in ENG.SENSITIVITY]

    def distribution(engine_ids, heading, note):
        print('\n## {}\n'.format(heading))
        print('| engine | titles | median | p90 | best | worst |')
        print('| :--- | ---: | ---: | ---: | ---: | ---: |')
        for engine_id in engine_ids:
            values = sorted(v for v in (cer(engine_id, k) for k in scored_items) if v is not None)
            if not values:
                continue
            print('| {} | {} | {} | {} | {} | {} |'.format(
                label(engine_id), len(values), _fmt(statistics.median(values), 1),
                _fmt(_pct(values, 90), 1), _fmt(values[0], 1), _fmt(values[-1], 1)))
        print('\n' + note)

    distribution(
        main_order,
        'Accuracy — the distribution over {} scored titles'.format(len(scored_items)),
        'Track-level CER. **p90 against median is the consistency column** — an engine that wins '
        'titles and then reads one at 30% is not a tool anyone can point at a library.')

    print('\n## Head to head — titles won, and titles too close to call\n')
    # A gap smaller than the title's own sidecar spread is not a result; that rule predates this
    # table and is why the corpus records `selector_spread_track_points` per item. What is new is
    # that twenty titles let the inconclusive ones be counted rather than swallowing the verdict.
    print('| subtrackt-fitted vs | wins | losses | too close to call |')
    print('| :--- | ---: | ---: | ---: |')
    for engine_id in main_order:
        if engine_id == 'subtrackt-fitted':
            continue
        wins = losses = tied = 0
        for key in scored_items:
            mine, theirs = cer('subtrackt-fitted', key), cer(engine_id, key)
            if mine is None or theirs is None:
                continue
            spread = items[key].get('selector_spread_track_points') or 0.0
            # `<=`, not `<`. A title with one candidate sidecar has a spread of 0.0, and with `<` a
            # gap of exactly zero fell through to the else and was recorded as a loss -- which is how
            # `subtrackt-fitted` first showed 9 losses to `subtrackt-arial` while producing output
            # byte-identical to it on all 26 items. Two engines that agree exactly have not lost.
            if abs(mine - theirs) <= spread:
                tied += 1
            elif mine < theirs:
                wins += 1
            else:
                losses += 1
        if wins or losses or tied:
            print('| {} | {} | {} | {} |'.format(label(engine_id), wins, losses, tied))

    distribution(
        sens_order,
        'Sensitivity arms — the same distribution, over the {} items they run on'.format(
            len(ENG.SENSITIVITY_ITEMS)),
        'Separated from the table above because these arms run on `{}` only, so their median is '
        'not comparable with a main arm\'s and putting them in one table invites exactly that '
        'comparison.'.format('`, `'.join(ENG.SENSITIVITY_ITEMS)))

    print('\n## Appendix — cue level (`all`) and track level per title, one pinned sidecar each\n')
    print('| item | sidecar spread | ' + ' | '.join(label(e).replace('|', '/') for e in main_order)
          + ' |')
    print('| :--- | ---: | ' + ' | '.join('---:' for _ in main_order) + ' |')
    for key in scored_items:
        cells = []
        for engine_id in main_order:
            cue, trk = cer(engine_id, key, 'all'), cer(engine_id, key)
            cells.append('{:.1f}% / {:.1f}%'.format(cue, trk) if trk is not None else '—')
        spread = items[key].get('selector_spread_track_points')
        print('| {} | {} | {} |'.format(
            key, _fmt(spread, 1) if spread is not None else '—', ' | '.join(cells)))

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

    # Only `cost` items. Every other item runs once (#209), and a single-run wall figure is a
    # lottery ticket for at least one engine in this table: `pgstosrt` spread 278% across three
    # repeats of `deathrace2`, with CPU-seconds moving with it. Printing an unrepeated figure here
    # would be exactly the quiet lie `bench.py`'s docstring is written against.
    print('\n## Time — median wall, spread, CPU seconds, %CPU\n')
    print('Drawn from the {} repeated `cost` items only: {}. No timing figure in this document '
          'comes from an item measured once.\n'.format(len(cost_items), ', '.join(cost_items)))
    print('| engine | item | wall s | spread s | cpu s | %cpu | rss MB | cgroup MB | n |')
    print('| :--- | :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |')
    for engine_id in order:
        for key in cost_items:
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
            print('| {} | {} | {} | {} | {} | {} | {} | {} | {} |'.format(
                label(engine_id), key,
                _fmt(_median(walls)), _fmt(_spread(walls)), _fmt(_median(cpus)),
                _fmt(_median(pcts), 0),
                _fmt((_median(rss) or 0) / 1024, 1),
                _fmt((_median(peaks) or 0) / 1048576, 1) if peaks else '—',
                len(reps)))

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

    # Scoped out loud. Since #209 most items run once, so byte-identity can only be *asserted* over
    # the repeated ones -- and a determinism claim that quietly covered items measured a single time
    # would be vacuous rather than merely narrow.
    #
    # Every item that HAS repeats, not just the `cost` five: the corpus was six items with three
    # repeats each before this issue, so `fixture` still carries its second and third runs. Those
    # are real measurements and deleting them to tidy the directory would be throwing away evidence
    # for the very claim this section makes.
    repeated = sorted({key for (_, key), reps in grouped.items() if len(reps) > 1})
    print('\n## Determinism — SHA-256 of output across repeats\n')
    pairs = disagreeing = 0
    rows_out = []
    for engine_id in order:
        for key in repeated:
            reps = grouped.get((engine_id, key), [])
            digests = {r.get('out_sha256') for r in reps if r.get('out_sha256')}
            if len(reps) < 2:
                continue
            pairs += 1
            if len(digests) > 1:
                disagreeing += 1
                rows_out.append('| {} | {} | **{}** |'.format(label(engine_id), key, len(digests)))
    print('Checked over the repeated `cost` items only: **{} (engine, item) pairs**, {}.\n'.format(
        pairs, '**{} with more than one distinct output**'.format(disagreeing) if disagreeing
        else 'every one byte-identical across its repeats'))
    if rows_out:
        print('| engine | item | distinct outputs |')
        print('| :--- | :--- | ---: |')
        print('\n'.join(rows_out))

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
    # Predictions 1 and 2 are cost claims, so they read the repeated items and nothing else, for
    # the reason `bench.py`'s docstring gives. #131 could iterate the whole corpus here because
    # every item was repeated; since #209 most are not.
    cost_items = [k for k, v in items.items() if v.get('cost')]

    def walls(engine_id, key):
        return [r.get('wall_s') for r in grouped.get((engine_id, key), [])]

    def cpus(engine_id, key):
        return [r.get('cpu_s') for r in grouped.get((engine_id, key), [])]

    def rss_mb(engine_id):
        out = []
        for key in cost_items:
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
    for key in cost_items:
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
    print('| engine | peak RSS MB (max over the {} repeated items) |'.format(len(cost_items)))
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
    print('\nTrack-level CER, one run per title -- every engine\'s output is byte-identical across '
          'repeats, which #209 measured over 50 (engine, item) pairs. **A gap smaller than that '
          'title\'s sidecar spread is not a result**: the spread is the instrument\'s own '
          'uncertainty.')

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

    _predictions_209(grouped, items, scored_items, cost_items, competitors)


def _predictions_209(grouped, items, scored_items, cost_items, competitors):
    """#209's predictions, committed to before the corpus widened. Same house rule as #131's."""

    def cer(engine_id, key):
        got = [r['scored'] for r in grouped.get((engine_id, key), []) if r.get('scored')]
        return _median([g['track']['cer'] for g in got]) if got else None

    def spread_of(engine_id):
        values = sorted(v for v in (cer(engine_id, k) for k in scored_items) if v is not None)
        return values

    print('\n### #209 predictions, scored\n')

    print('**1. The head-to-head resolves: some engine wins a majority of titles by margins that '
          'survive the per-title sidecar spread.**\n')
    print('| engine | titles won outright | titles too close to call |')
    print('| :--- | ---: | ---: |')
    for engine_id in [e['id'] for e in ENG.ENGINES]:
        won = close = 0
        for key in scored_items:
            mine = cer(engine_id, key)
            if mine is None:
                continue
            others = [v for v in (cer(o, key) for o in [e['id'] for e in ENG.ENGINES]
                                  if o != engine_id) if v is not None]
            if not others:
                continue
            spread = items[key].get('selector_spread_track_points') or 0.0
            best_other = min(others)
            if mine < best_other and (best_other - mine) > spread:
                won += 1
            elif abs(mine - best_other) <= spread:
                close += 1
        print('| {} | {} | {} |'.format(ENG.by_id(engine_id)['label'], won, close))

    print('\n**2. subtrackt-arial degrades by more than 5 points of median CER against its '
          'three-disc figure; subtrackt-fitted degrades by less than 2.**\n')
    print('| arm | median CER over the wide corpus | over clover/wanda/gonegirl | delta |')
    print('| :--- | ---: | ---: | ---: |')
    anchors = [k for k in ('clover', 'wanda', 'gonegirl') if k in scored_items]
    for engine_id in ('subtrackt-fitted', 'subtrackt-arial'):
        wide = spread_of(engine_id)
        old = sorted(v for v in (cer(engine_id, k) for k in anchors) if v is not None)
        if not wide or not old:
            continue
        print('| {} | {} | {} | {} |'.format(
            ENG.by_id(engine_id)['label'], _fmt(statistics.median(wide), 1),
            _fmt(statistics.median(old), 1),
            _fmt(statistics.median(wide) - statistics.median(old), 1)))

    print('\n**3. No competitor is good on all of them either: every Tesseract wrapper has a p90 '
          'more than 3x its median.**\n')
    print('| engine | median | p90 | ratio |')
    print('| :--- | ---: | ---: | ---: |')
    for engine_id in competitors:
        values = spread_of(engine_id)
        if len(values) < 2:
            continue
        med, p90 = statistics.median(values), _pct(values, 90)
        print('| {} | {} | {} | {} |'.format(
            ENG.by_id(engine_id)['label'], _fmt(med, 1), _fmt(p90, 1),
            _fmt(p90 / med, 1) if med else '—'))

    print('\n**4. Determinism survives: the cost subset stays byte-identical across its repeats.** '
          'See the determinism section of `tables`.\n')

    print('**5. pgstosrt remains the only engine with a cost spread over 20% on the repeated '
          'subset.**\n')
    print('| engine | item | wall spread as % of median |')
    print('| :--- | :--- | ---: |')
    for engine_id in [e['id'] for e in ENG.all_engines()]:
        for key in cost_items:
            walls = [r.get('wall_s') for r in grouped.get((engine_id, key), [])
                     if r.get('wall_s')]
            if len(walls) < 2:
                continue
            med = statistics.median(walls)
            pct_spread = (max(walls) - min(walls)) / med * 100 if med else 0
            if pct_spread > 20:
                print('| {} | {} | **{:.0f}%** |'.format(ENG.by_id(engine_id)['label'], key,
                                                         pct_spread))


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
