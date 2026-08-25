# The documentation site

Published at
[sovereign-media.github.io/Sovereign.SubTrackt](https://sovereign-media.github.io/Sovereign.SubTrackt/)
by `.github/workflows/docs.yml`, on a push to `main` that touches `docs/`, `site/` or the workflow.
Pull requests build it without deploying.

```console
$ npm ci
$ npm run dev        # http://localhost:3000/Sovereign.SubTrackt/
$ npm test           # the link rewrite, and what the guide is not allowed to say
$ npm run build      # generate, render to static HTML, follow every link, check the stylesheet
$ npm run typecheck  # after a build: the route files it checks are written by one
```

**This is outside the Cargo workspace, and that is the point.** A Node toolchain must never be a
prerequisite for `cargo build`, and `scripts/check.sh` does not grow a step for it. The dependency
discipline in `CLAUDE.md` is about what gets linked into the binary; a static-site generator that
runs in CI and emits HTML is not that, and `cargo tree -p subtrackt -e normal` is unchanged.

## Where the content lives

| | |
| :--- | :--- |
| `content/guide/` | Why the tool works the way it does, written by hand and committed. No prior knowledge assumed, no CER figures, no issue numbers — `tests/` enforces the last two. |
| `content/usage/` | The command reference, one page per subcommand, in the order you would run them. |
| `src/content/` | Generated from both by `scripts/build-content.mjs`, and gitignored. Never edit this. |
| `site.json` | Sidebar order. The one list; the generator fails the build if it disagrees with what is on disk. |

`../docs/` is **not** part of the site. The fourteen documents there are the record of what was
measured and why each decision went the way it did, and they are read in the repository: the guide
says what they found in words that assume nothing, and links out to the document for the workings.

`scripts/content.mjs` is where the work is. It puts the `/Sovereign.SubTrackt/` prefix on a
site-absolute link, sends a `../` target off to GitHub, renames ```` ```console ```` fences, which
GitHub knows and Prism does not, and **fails the build on a link it cannot resolve or an anchor no
heading answers**. `scripts/check-links.mjs` then follows every link through the rendered HTML,
which is the half that catches a page the prerenderer never wrote.

## The corpus is still checked, even though it is not built

`content.mjs` parses `docs/` on every build and holds it to the rules GitHub reads it by: a sibling
named by filename has to exist, and a `#fragment` has to name a real heading — in a document
linking to a sibling, and in a guide page linking out.

That is deliberate and it is the one thing not to drop when touching this. Those fifty-odd
cross-links used to be checked because the corpus was compiled into pages. It is not any more, so
without this a document renamed here would break every referrer silently, which is the exact failure
the rest of this file exists to prevent.

## Adding a research document

Write it in `docs/` and link to it. Nothing has to be registered anywhere: it is not a page, so it
needs no sidebar entry, no label and no description. A guide page reaches it as
`../docs/whatever.md`, and the build checks that the file and the anchor are both real.

## The three things the browser does

`src/components/` holds enhancements applied to the rendered markdown after it reaches the page,
mounted once for the whole site in `src/routes/__root.tsx`. They are in the browser because there
is nowhere else to put them: the route file that renders a page is generated into
`src/routes/(prestige)/` and gitignored, so there is no MDX `components` map to hand anything to,
and `markdown.rehypePlugins` is the no-op described below.

| | |
| :--- | :--- |
| `lightbox.tsx` | Click a diagram to see it at about 1.7x. The diagrams are authored 640 wide to suit the prose column, which is the right size to read past and a small one to read into. |
| `heading-links.tsx` | The `#` link on every heading. `rehype-slug` already puts the `id` there — this is the only way a reader can get at it. `rehype-autolink-headings` is the right tool and cannot be installed. |

Both walk `.prose` on mount and again on a `MutationObserver`, because client-side navigation
swaps the article without remounting the root. Both render nothing on the server, so the
prerendered HTML is byte-identical to what it was before they existed.

Inline code is the third, and it is only CSS. `@tailwindcss/typography` draws literal backticks
around a `<code>` span with `::before`/`::after`; `src/styles.css` turns that off and gives code a
background instead. `scripts/check-styles.mjs` asserts it over the built stylesheet on every build,
because the way that regresses is a dependency upgrade rather than an edit to this repository.

## Notes on the framework

[Prestige](https://github.com/lukonik/prestige) is alpha and small. If it stalls, what this
repository owns is still plain markdown in two directories, and the migration is pointing a
different generator at them.

Two things about 0.15.0 worth knowing before changing anything here:

- **`markdown.rehypePlugins` in `prestige.config.ts` does nothing.** It is validated, and documented
  in Prestige's own config reference, and never passed to the compiler. Both rewrites the site needs
  are in the generator for that reason, and it is why `src/components/` exists — see below.
- **Search is off, and omitting the `algolia` key is the whole of turning it off.** There is no
  Algolia application, no crawler to register, and no index to go stale against a document that
  changed.
