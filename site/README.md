# The documentation site

Published at
[sovereign-media.github.io/Sovereign.SubTrackt](https://sovereign-media.github.io/Sovereign.SubTrackt/)
by `.github/workflows/docs.yml`, on a push to `main` that touches `docs/`, `site/` or the workflow.
Pull requests build it without deploying.

```console
$ npm ci
$ npm run dev        # http://localhost:3000/Sovereign.SubTrackt/
$ npm test           # the link rewrite, and what the guide is not allowed to say
$ npm run build      # generate, render to static HTML, then follow every link in the output
$ npm run typecheck  # after a build: the route files it checks are written by one
```

**This is outside the Cargo workspace, and that is the point.** A Node toolchain must never be a
prerequisite for `cargo build`, and `scripts/check.sh` does not grow a step for it. The dependency
discipline in `CLAUDE.md` is about what gets linked into the binary; a static-site generator that
runs in CI and emits HTML is not that, and `cargo tree -p subtrackt -e normal` is unchanged.

## Where the content lives

| | |
| :--- | :--- |
| `content/guide/` | The plain-language half, written by hand and committed. No prior knowledge assumed, no CER figures, no issue numbers — `tests/` enforces the last two. |
| `../docs/` | The research half. **Canonical, and edited there.** The README links into it twenty-odd times and every one of those URLs is one somebody may already have. |
| `src/content/` | Generated from both by `scripts/build-content.mjs`, and gitignored. Never edit this. |
| `site.json` | Sidebar order, groups, and the label and description of each research document. The one list; the generator fails the build if it disagrees with what is on disk. |

`scripts/content.mjs` is where the work is. It rewrites `reference-set.md`, which is right on GitHub
and a 404 here, into `/Sovereign.SubTrackt/research/reference-set`; sends `../crates/...` off to
GitHub, which has no route here at all; renames ```` ```console ```` fences, which GitHub knows and
Prism does not; and **fails the build on a link it cannot resolve or an anchor no heading answers**.
`scripts/check-links.mjs` then follows every link through the rendered HTML, which is the half that
catches a page the prerenderer never wrote.

## Adding a research document

Write it in `docs/` as usual, then add one line to `site.json` naming its group, its sidebar label
and a one-sentence description. The build fails until you do, which is deliberate: a document with
no group has nowhere to appear.

## Notes on the framework

[Prestige](https://github.com/lukonik/prestige) is alpha and small. If it stalls, what this
repository owns is still plain markdown in two directories, and the migration is pointing a
different generator at them.

Two things about 0.15.0 worth knowing before changing anything here:

- **`markdown.rehypePlugins` in `prestige.config.ts` does nothing.** It is validated, and documented
  in Prestige's own config reference, and never passed to the compiler. Both rewrites the site needs
  are in the generator for that reason.
- **Search is off, and omitting the `algolia` key is the whole of turning it off.** There is no
  Algolia application, no crawler to register, and no index to go stale against a document that
  changed.
