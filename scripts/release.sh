#!/usr/bin/env bash
# Prepare a release: bump the version everywhere it appears, prove the tree still builds, and open
# the pull request that lands it.
#
# This exists because the version lives in seven places, not one. `[workspace.package]` sets it, and
# each of the six path dependencies in `[workspace.dependencies]` repeats it as a constraint — so
# bumping only the first leaves Cargo unable to resolve the workspace, and bumping by hand is a
# transcription exercise with six chances to get it wrong.
#
# It deliberately stops short of tagging. `CLAUDE.md` says work lands through a pull request rather
# than a push to main, and a tag is what triggers the release build, so the tag has to come after
# the bump is merged. The command to run then is printed at the end.
#
# Usage: scripts/release.sh 0.2.0
set -euo pipefail

cd "$(dirname "$0")/.."

version="${1:-}"
if [ -z "$version" ]; then
  echo "usage: scripts/release.sh <version>, e.g. scripts/release.sh 0.2.0" >&2
  exit 2
fi

# Leading `v` is how the tag is spelled, not how the manifest is. Accept either and normalise, so
# `scripts/release.sh v0.2.0` does not produce a version of "v0.2.0".
version="${version#v}"

# Semver with an optional pre-release suffix. Rejected here rather than by Cargo three steps later,
# and the release workflow reads the same suffix to decide whether the tag is a pre-release.
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$'; then
  echo "'$version' is not a semantic version" >&2
  exit 2
fi

current=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
if [ "$version" = "$current" ]; then
  echo "already at $version" >&2
  exit 2
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "working tree is not clean; commit or stash first" >&2
  exit 2
fi

branch="release-$version"
echo "== bumping $current -> $version =="

git switch -c "$branch" >/dev/null 2>&1

# Line 1 of 7: the workspace version itself.
sed -i "0,/^version = \"$current\"/s//version = \"$version\"/" Cargo.toml

# Lines 2-7: each path dependency repeats the version as a constraint. Anchored on the crate name so
# this cannot touch a third-party entry that happens to share a version number.
sed -i -E "s|^(subtrackt[a-z-]* = \{ path = \"[^\"]*\", version = )\"$current\"|\1\"$version\"|" Cargo.toml

changed=$(grep -c "\"$version\"" Cargo.toml || true)
if [ "$changed" -ne 7 ]; then
  echo "expected 7 occurrences of $version in Cargo.toml, found $changed" >&2
  echo "the manifest layout changed; fix this script rather than the manifest" >&2
  git switch - >/dev/null 2>&1
  git branch -D "$branch" >/dev/null 2>&1
  exit 1
fi

# Rewrites Cargo.lock to match. Without this the `--locked` builds in CI fail on the very first
# step, which is a confusing way to find out about a version bump.
cargo check --workspace --quiet

echo "== running the full gate =="
scripts/check.sh

git commit -aqm "Release $version

Bumps the workspace version and the six path-dependency constraints that
repeat it. Tagging happens after this lands: the tag is what triggers the
release build, and it has to name a commit that is already on main."

git push -q -u origin "$branch"

gh pr create \
  --title "Release $version" \
  --body "Version bump only: \`[workspace.package]\` and the six path-dependency constraints that repeat it.

Tag once this is merged, which is what builds and publishes the artifacts:

\`\`\`console
git checkout main && git pull
git tag v$version && git push origin v$version
\`\`\`"

cat <<EOF

$(printf '\033[1;32m')prepared $version$(printf '\033[0m')

Merge the pull request above, then:

  git checkout main && git pull
  git tag v$version && git push origin v$version

The tag triggers .github/workflows/release.yml, which verifies the tag against
Cargo.toml, builds four artifacts, and publishes them with generated notes.
EOF
