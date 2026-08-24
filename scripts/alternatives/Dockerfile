# The bench image for #131: five tools over one set of bytes.
#
# `ubuntu:24.04` deliberately, and not because it is small. sovereign#328 -- the analysis that
# motivated building this library, and the only prior timing this project can be cross-compared
# against -- ran on Noble, and took its ffmpeg 6.1.1, Tesseract 5.3.4 and leptonica 1.82.0 from
# Noble's archive. Matching the environment is worth more than saving a few hundred megabytes,
# because the alternative is a cost comparison whose two halves were measured on different builds
# of the engine under test.
#
# Everything is pinned by version *and* by checksum. #328's lasting value is largely that it named
# its versions; an image that resolves `latest` at build time cannot be re-run to the same numbers,
# and a benchmark that cannot be re-run is an anecdote with a table in it.
#
# Nothing here may reach a workspace manifest. This is the first thing in the repository to pull
# Python, .NET and apt packages, and it lives entirely under `scripts/` for that reason:
# `scripts/check.sh`'s dependency-discipline step must still report exactly `adler2 miniz_oxide`
# after this file exists.

# ---------------------------------------------------------------------------------------------
# subtrackt, built for musl from the tree being measured.
#
# `docs/distribution.md` sets the precedent of quoting the *published* binary, and this
# deliberately departs from it -- with the reason recorded rather than the rule quietly broken.
# v0.0.3-alpha was tagged at b36a5c3, and #186, #187 and #188 landed after it. #188 is the one that
# matters: it flipped `Config::post_correct` and `Config::lone_words` to `true`. Benchmarking the
# published binary would therefore score subtrackt's *previous* defaults against every competitor's
# current ones, and §(h) of the issue makes default configuration the headline. The commit actually
# built is stamped into /opt/manifest/versions.txt, and the released binary's behaviour is not lost:
# it is exactly the `--no-post-correct` sensitivity arm.
FROM rust:1.98.0-alpine3.22 AS subtrackt-build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask
# The workspace lists xtask as a member, so its manifest has to be present for Cargo to resolve at
# all; only the CLI is built.
RUN cargo build --release --locked -p subtrackt-cli \
 && strip target/release/subtrackt

# ---------------------------------------------------------------------------------------------
FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive

# `time` is the instrument, not a convenience: bash's builtin `time` cannot report maximum RSS and
# has no `-o`, and §(l) needs the measurement written to a file rather than interleaved with a
# tool's own stderr.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      time \
      tesseract-ocr \
      tesseract-ocr-eng \
      mkvtoolnix \
      dotnet-runtime-8.0 \
      python3 \
      python3-venv \
      libicu74 \
      ca-certificates \
      libgl1 \
      libglib2.0-0t64 \
 && rm -rf /var/lib/apt/lists/*

# Tesseract's best shot, from a pinned commit rather than a clone. `git clone tessdata_best` is
# about 2 GB and pins nothing; one file by checksum is 15 MB and pins exactly what was measured.
ADD --checksum=sha256:8280aed0782fe27257a68ea10fe7ef324ca0f8d85bd2fd145d1c2b560bcb66ba \
    https://raw.githubusercontent.com/tesseract-ocr/tessdata_best/e12c65a915945e4c28e237a9b52bc4a8f39a0cec/eng.traineddata \
    /opt/tessdata-best/eng.traineddata

# Subtitle Edit's converter. Self-contained: the Linux tarball is the binary plus three native
# libraries, and -- checked rather than assumed, because prediction 6 rested on it -- it carries no
# OCR database, no dictionary and no `Ocr.zip`. The desktop tarball carries none either.
ADD --checksum=sha256:3354dfeb4452b0dd8a11d8f3066c31a3116e2cbc137f271ac6b7e4f91896ba11 \
    https://github.com/SubtitleEdit/subtitleedit/releases/download/v5.1.0/SeConv-Linux-x64.tar.gz \
    /tmp/seconv.tar.gz
RUN mkdir -p /opt/seconv && tar xzf /tmp/seconv.tar.gz -C /opt/seconv && rm /tmp/seconv.tar.gz \
 && chmod +x /opt/seconv/seconv

# The OCR databases the converter does not ship, from the source tree that does. Both arms can
# therefore run unattended after one fetch -- which is the finding, not a workaround: the issue
# predicted "cannot be run unattended at all" and the artefacts say otherwise. Size and checksum are
# recorded because `OcrInitializer.NeedsUpdate` treats an unpacked `Latin.nocr` under 1,700,000
# bytes as stale and this one is 486,979, so the database actually used has to be named.
ADD --checksum=sha256:e10200c7c7937d229c8f521e747b44ebd9eee15e836ba8cae00bc5efed96299d \
    https://raw.githubusercontent.com/SubtitleEdit/subtitleedit/cc1e6b5416f3966d8c094e85ba9ca27dcc9f2837/Ocr/Latin.nocr \
    /opt/se-ocr/Latin.nocr
ADD --checksum=sha256:dff65d2d9aeb0bb5d9cc6fc251491b25abe1292270a2691477b4ef5237eb1a55 \
    https://raw.githubusercontent.com/SubtitleEdit/subtitleedit/cc1e6b5416f3966d8c094e85ba9ca27dcc9f2837/Ocr/Latin.db \
    /opt/se-ocr/Latin.db

# A third Tesseract wrapper, framework-dependent, hence the .NET runtime above.
ADD --checksum=sha256:27c3e637fe777cabe55b063a5a454e124c395e727d6a270899be3b5a7b2a9c7a \
    https://github.com/Tentacule/PgsToSrt/releases/download/v1.4.8/PgsToStr-1.4.8.zip \
    /tmp/pgstosrt.zip
RUN apt-get update && apt-get install -y --no-install-recommends unzip \
 && mkdir -p /opt/pgstosrt && unzip -q /tmp/pgstosrt.zip -d /opt/pgstosrt && rm /tmp/pgstosrt.zip \
 && apt-get purge -y unzip && apt-get autoremove -y && rm -rf /var/lib/apt/lists/*

# The fourth, as people actually run it in pipelines.
# `libgl1` and `libglib2.0-0t64` are in the apt list above for this line alone: pgsrip depends on
# `opencv-python` rather than the headless build, so importing it pulls in the full OpenGL stack.
# Step 0 caught this -- `import cv2` raised `libGL.so.1: cannot open shared object file` -- which
# is what a smoke gate before the harness exists is for. It is a cost as well as a fix: two X11
# libraries in a headless container are part of what this tool asks a pipeline to install, and
# they are counted in the install-tree row rather than waved through.
RUN python3 -m venv /opt/pgsrip-venv \
 && /opt/pgsrip-venv/bin/pip install --no-cache-dir pgsrip==0.1.12 \
 && /opt/pgsrip-venv/bin/pip freeze > /opt/manifest-pip.txt

COPY --from=subtrackt-build /src/target/release/subtrackt /opt/subtrackt/subtrackt

# PgsToSrt reads `--tesseractdata` as a directory of `.traineddata`; pgsrip reads TESSDATA_PREFIX.
# Both point at the same pinned file so "tessdata_best" means one artefact across every arm.
ENV TESSDATA_BEST=/opt/tessdata-best

# Install-tree sizes become table rows in `docs/alternatives.md`, so they are measured here, once,
# against the tree that shipped -- rather than quoted from a project's README.
ARG SUBTRACKT_COMMIT=unknown
RUN mkdir -p /opt/manifest && mv /opt/manifest-pip.txt /opt/manifest/pip-freeze.txt \
 && { for d in /opt/seconv /opt/pgstosrt /opt/pgsrip-venv /opt/subtrackt /opt/tessdata-best /opt/se-ocr \
                /usr/share/tesseract-ocr /usr/lib/dotnet; do \
        [ -e "$d" ] && printf '%s\t%s\n' "$(du -sb "$d" | cut -f1)" "$d"; \
      done; } > /opt/manifest/sizes.txt \
 && { echo "image-built-from-commit: ${SUBTRACKT_COMMIT}"; \
      echo "ubuntu: $(. /etc/os-release; echo "$PRETTY_NAME")"; \
      echo "tesseract: $(tesseract --version 2>&1 | head -1)"; \
      echo "leptonica: $(tesseract --version 2>&1 | grep -i leptonica | head -1)"; \
      echo "dotnet: $(dotnet --list-runtimes 2>&1 | head -1)"; \
      echo "python: $(python3 --version 2>&1)"; \
      echo "pgsrip: $(/opt/pgsrip-venv/bin/pip show pgsrip 2>/dev/null | awk '/^Version:/{print $2}')"; \
      echo "seconv: $(/opt/seconv/seconv --version 2>&1 | head -1)"; \
      echo "subtrackt: $(/opt/subtrackt/subtrackt --version 2>&1 | head -1)"; \
      echo "mkvtoolnix: $(mkvextract --version 2>&1 | head -1)"; \
      echo "latin-nocr-bytes: $(stat -c%s /opt/se-ocr/Latin.nocr)"; \
      echo "latin-nocr-sha256: $(sha256sum /opt/se-ocr/Latin.nocr | cut -d' ' -f1)"; \
      echo "latin-db-bytes: $(stat -c%s /opt/se-ocr/Latin.db)"; \
      echo "latin-db-sha256: $(sha256sum /opt/se-ocr/Latin.db | cut -d' ' -f1)"; \
      echo "eng-best-sha256: $(sha256sum /opt/tessdata-best/eng.traineddata | cut -d' ' -f1)"; \
      echo "eng-apt-sha256: $(sha256sum /usr/share/tesseract-ocr/*/tessdata/eng.traineddata 2>/dev/null | head -1 | cut -d' ' -f1)"; \
    } > /opt/manifest/versions.txt

WORKDIR /work
