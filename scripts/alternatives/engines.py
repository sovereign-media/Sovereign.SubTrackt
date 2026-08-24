"""The engine table for #131.

Python rather than JSON so the fairness argument can sit in a comment next to the flag it is about.
Every one of these flags was chosen for a reason, several of them against our own interest, and a
JSON file would have carried the flags and lost the reasons.

Two rules hold across the whole table.

**Every engine reads the same flat `.sup`.** `xtask dump-sup` is byte-exact -- `docs/error-census.md`
records that extracting the rip and extracting the dump produce byte-identical subtitles -- so no
demux difference can contaminate an accuracy or a timing figure. Nothing here is given an `.mkv`.

**The headline arm of each tool is its own default configuration**, section (h). Where a tool ships
a corrector on, it runs on; where a tool ships one off, it stays off; and each is also run the other
way as a sensitivity arm, with the number of cues the corrector changed reported. That asymmetry is
not the one #131 predicted, and the reversal is worth stating rather than quietly accommodating:
the issue was written expecting Subtitle Edit to run an OCR-fix pass and subtrackt's corrector to be
off. Both are now the other way round. `seconv`'s FCE pass is opt-in (`--fix-common-errors`, and the
OCR replace list needs `--dictionary-folder` besides), and #188 flipped `Config::post_correct` and
`Config::lone_words` to true after v0.0.3-alpha was tagged.
"""

# Where things live inside the image. The corpus mount is read-only; everything an engine writes
# goes to the tmpfs at /tmp and is copied out after `time` has exited.
SUBTRACKT = '/opt/subtrackt/subtrackt'
SECONV = '/opt/seconv/seconv'
PGSTOSRT = '/opt/pgstosrt/PgsToSrt.dll'
PGSRIP = '/opt/pgsrip-venv/bin/pgsrip'

TESSDATA_APT = '/usr/share/tesseract-ocr/5/tessdata'
TESSDATA_BEST = '/opt/tessdata-best'

CORPUS = '/corpus'
OUT = '/tmp/out.srt'


def _sh(*lines):
    """Join shell lines into one `sh -c` body."""
    return '\n'.join(lines)


# ---------------------------------------------------------------------------------------------
# The table.
#
# `cmd` is a shell body run under `/usr/bin/time -v`. It must leave its SubRip output at /tmp/out.srt
# and must not write anywhere outside /tmp. `{sup}` is the corpus `.sup`; `{key}` is the item key.
#
# `sensitivity` marks an arm that runs on `fixture` and `clover` only, per the issue's scope. The
# main table is the arms without it.
ENGINES = [
    # -----------------------------------------------------------------------------------------
    # subtrackt.
    {
        'id': 'subtrackt-fitted',
        'tool': 'subtrackt',
        'family': 'glyph-match',
        'label': 'subtrackt, fitted set',
        # `fit` is INSIDE the timed region, and this is the single most important flag decision in
        # the file. The fitted arm is the headline speed claim; if the fitting pass that makes it
        # possible were excluded, the number would be wrong in our favour and it is the first thing
        # a hostile reader would check. `fit` scans 400 cues by default, which is its own shipped
        # default and is what a user would pay.
        'cmd': _sh(
            '{S} fit {sup} --references {C}/refs -o /tmp/fitted.subtref --plain',
            '{S} extract {sup} --reference /tmp/fitted.subtref -o {OUT} --plain',
        ),
        'corrector': 'default-on',
    },
    {
        'id': 'subtrackt-arial',
        'tool': 'subtrackt',
        'family': 'glyph-match',
        'label': 'subtrackt, one Arial set',
        # One set for every item, no per-title work: the condition `docs/library-accuracy.md` was
        # measured under, and the arm that answers "what does a set that happens to be right cost".
        'cmd': '{S} extract {sup} --reference {C}/sets/arial-ri.subtref -o {OUT} --plain',
        'corrector': 'default-on',
    },
    {
        'id': 'subtrackt-liberation',
        'tool': 'subtrackt',
        'family': 'glyph-match',
        'label': 'subtrackt, one Liberation set',
        # Question 2, and the honest one: point it at a track you know nothing about. subtrackt has
        # no out-of-the-box configuration -- nothing is embedded, which is a documented Shortcoming
        # -- so the nearest thing to one is the set a distributor could actually ship. Liberation
        # Sans is OFL 1.1, metrically compatible with Arial, and `docs/reference-set.md` already
        # prices it at eleven points worse than the ceiling. This arm is expected to lose.
        'cmd': '{S} extract {sup} --reference {C}/sets/liberation-ri.subtref -o {OUT} --plain',
        'corrector': 'default-on',
    },

    # -----------------------------------------------------------------------------------------
    # Subtitle Edit's converter, three engines.
    {
        'id': 'seconv-tesseract',
        'tool': 'seconv',
        'family': 'tesseract',
        'label': 'seconv, Tesseract (apt tessdata)',
        # What a user gets by default: the distribution's own `eng.traineddata`.
        'cmd': _sh(
            'TESSDATA_PREFIX={TA} {SE} {sup} subrip --ocr-engine:tesseract --ocr-language:eng'
            ' --outputfilename:{OUT}',
        ),
        'corrector': 'default-off',
    },
    {
        'id': 'seconv-tesseract-best',
        'tool': 'seconv',
        'family': 'tesseract',
        'label': 'seconv, Tesseract (tessdata_best)',
        # Tesseract's best shot. Included because benchmarking an engine on its weaker model and
        # reporting the result as the engine's accuracy would be the mirror image of scoring
        # subtrackt only on a fitted set.
        'cmd': _sh(
            'TESSDATA_PREFIX={TB} {SE} {sup} subrip --ocr-engine:tesseract --ocr-language:eng'
            ' --outputfilename:{OUT}',
        ),
        'corrector': 'default-off',
    },
    {
        'id': 'seconv-nocr',
        'tool': 'seconv',
        'family': 'glyph-match',
        'label': 'seconv, nOCR (generic Latin)',
        # The direct design peer: a trained glyph matcher rather than a language model. #131
        # predicted this arm could not run unattended at all, because `seconv` ships no database.
        # It ships none -- verified against the artefact, not the README: the Linux tarball is one
        # binary and three native libraries -- but the database is in the source tree at
        # `Ocr/Latin.nocr`, fetched here by checksum. So the arm runs, with no human in it, and the
        # prediction's first clause is dead. Its second clause is the whole prediction now: a
        # generic database is nobody's typeface.
        'cmd': _sh(
            '{SE} {sup} subrip --ocr-engine:nocr --ocr-db:/opt/se-ocr/Latin.nocr'
            ' --outputfilename:{OUT}',
        ),
        'corrector': 'default-off',
    },
    {
        'id': 'seconv-binaryocr',
        'tool': 'seconv',
        'family': 'image-compare',
        'label': 'seconv, binary image compare',
        'cmd': _sh(
            '{SE} {sup} subrip --ocr-engine:binaryocr --ocr-db:/opt/se-ocr/Latin.db'
            ' --outputfilename:{OUT}',
        ),
        'corrector': 'default-off',
    },

    # -----------------------------------------------------------------------------------------
    # The two standalone Tesseract wrappers.
    {
        'id': 'pgsrip',
        'tool': 'pgsrip',
        'family': 'tesseract',
        'label': 'pgsrip',
        # pgsrip identifies a bare `.sup` by a language suffix in its filename and refuses one
        # without it -- "1 file filtered out" -- so the copy into the tmpfs is named for the
        # language rather than for the item. It also insists on writing beside its input, which is
        # why it gets a directory of its own.
        #
        # tessdata_best, because pgsrip's own README tells the user to install it; running it on the
        # apt model would be benchmarking a configuration its author does not recommend.
        'cmd': _sh(
            'mkdir -p /tmp/pr && cp {sup} /tmp/pr/item.en.sup',
            'TESSDATA_PREFIX={TB} {PR} -l en --force /tmp/pr/item.en.sup',
            'cp /tmp/pr/item.en.srt {OUT} 2>/dev/null || true',
        ),
        'corrector': 'default-on',
    },
    {
        'id': 'pgstosrt',
        'tool': 'pgstosrt',
        'family': 'tesseract',
        'label': 'PgsToSrt',
        # `--tesseractversion 5` is not a tuning choice, it is the only way this tool runs at all on
        # a current distribution. It defaults to 4 and then loads `libtesseract.so.4`, which no
        # supported Ubuntu has shipped for years; out of the box on Ubuntu 24.04 it produces a
        # `DllNotFoundException` and no subtitles. That is a Table 1 fact, not a footnote, and it is
        # recorded there rather than silently fixed here.
        'cmd': _sh(
            'dotnet {P2S} --input {sup} --output {OUT} --tesseractlanguage eng'
            ' --tesseractdata {TB} --tesseractversion 5',
        ),
        'corrector': 'default-off',
    },
]

# ---------------------------------------------------------------------------------------------
# Sensitivity arms. `fixture` and `clover` only.
#
# These are not competitors; each one isolates a single variable against a main-table arm so the
# headline can be read knowing what it rests on.
SENSITIVITY = [
    {
        'id': 'subtrackt-arial-nopost',
        'tool': 'subtrackt',
        'family': 'glyph-match',
        'label': 'subtrackt, Arial set, corrector off',
        'against': 'subtrackt-arial',
        # Also the released v0.0.3-alpha's behaviour, since #188 flipped the default after the tag.
        # So this arm doubles as "what a user who downloads the binary today actually gets".
        'cmd': (
            '{S} extract {sup} --reference {C}/sets/arial-ri.subtref -o {OUT} --plain'
            ' --no-post-correct'
        ),
        'corrector': 'off',
    },
    {
        'id': 'seconv-tesseract-fce',
        'tool': 'seconv',
        'family': 'tesseract',
        'label': 'seconv, Tesseract, FCE on',
        'against': 'seconv-tesseract',
        # The other half of (h): a spell-corrected OCR output is precisely the confident-wrong-answer
        # failure mode the README argues about, so the count of cues this changes is the row where
        # that argument becomes visible rather than asserted.
        'cmd': _sh(
            'TESSDATA_PREFIX={TA} {SE} {sup} subrip --ocr-engine:tesseract --ocr-language:eng'
            ' --fix-common-errors --outputfilename:{OUT}',
        ),
        'corrector': 'on',
    },
    {
        'id': 'seconv-tesseract-1thread',
        'tool': 'seconv',
        'family': 'tesseract',
        'label': 'seconv, Tesseract, OMP_THREAD_LIMIT=1',
        'against': 'seconv-tesseract',
        # (i): subtrackt is single-threaded and Tesseract is not, so wall clock alone would flatter
        # Tesseract. This arm is what a queue actually pays for a slot.
        'cmd': _sh(
            'OMP_THREAD_LIMIT=1 TESSDATA_PREFIX={TA} {SE} {sup} subrip --ocr-engine:tesseract'
            ' --ocr-language:eng --outputfilename:{OUT}',
        ),
        'corrector': 'default-off',
    },
    {
        'id': 'pgstosrt-1thread',
        'tool': 'pgstosrt',
        'family': 'tesseract',
        'label': 'PgsToSrt, OMP_THREAD_LIMIT=1',
        'against': 'pgstosrt',
        'cmd': _sh(
            'OMP_THREAD_LIMIT=1 dotnet {P2S} --input {sup} --output {OUT} --tesseractlanguage eng'
            ' --tesseractdata {TB} --tesseractversion 5',
        ),
        'corrector': 'default-off',
    },
    {
        'id': 'pgsrip-1thread',
        'tool': 'pgsrip',
        'family': 'tesseract',
        'label': 'pgsrip, OMP_THREAD_LIMIT=1',
        'against': 'pgsrip',
        'cmd': _sh(
            'mkdir -p /tmp/pr && cp {sup} /tmp/pr/item.en.sup',
            'OMP_THREAD_LIMIT=1 TESSDATA_PREFIX={TB} {PR} -l en --force /tmp/pr/item.en.sup',
            'cp /tmp/pr/item.en.srt {OUT} 2>/dev/null || true',
        ),
        'corrector': 'default-on',
    },
]

SENSITIVITY_ITEMS = ('fixture', 'clover')


def render(engine, sup_path):
    """Fill an engine's command template in."""
    return engine['cmd'].format(
        S=SUBTRACKT, SE=SECONV, P2S=PGSTOSRT, PR=PGSRIP,
        TA=TESSDATA_APT, TB=TESSDATA_BEST,
        C=CORPUS, OUT=OUT, sup=sup_path,
    )


def all_engines():
    return list(ENGINES) + list(SENSITIVITY)


def by_id(engine_id):
    for e in all_engines():
        if e['id'] == engine_id:
            return e
    raise KeyError(engine_id)
