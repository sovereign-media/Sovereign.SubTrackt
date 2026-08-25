// The link rewrite is the only real logic in this site, and a broken cross-link is how a docs site
// rots quietly, so it is the thing with tests.
//
// `buildPages()` is run against the real corpus rather than against fixtures. That is deliberate:
// it means adding a document to `docs/` that links to a heading which does not exist fails here,
// on a pull request, rather than shipping. It matters more now than it did: `docs/` is no longer
// built into pages, so this and the corpus check it calls are the only things that read those
// links at all. `scripts/check-links.mjs` then follows the site's own links through the built
// HTML, which is what catches a page that was never rendered.

import { describe, expect, it } from "vitest";
import {
  buildPages,
  checkCorpus,
  checkFrontmatter,
  readAssets,
  readHeadings,
  rewriteFences,
  rewriteLinks,
  site,
} from "../scripts/content.mjs";

const PREFIX = site.base.replace(/\/$/, "");
const pages = await buildPages();

/** A rewrite context standing in for one guide page linking out to the corpus and to a sibling. */
function context(overrides: Record<string, unknown> = {}) {
  return {
    source: "content/guide/here.md",
    route: "/guide/here",
    routes: new Map([
      ["/guide/here", new Set(["own-heading"])],
      ["/guide/what-this-is", new Set<string>()],
    ]),
    repo: new Map([["docs/there.md", new Set(["a-heading"])]]),
    assets: new Set(["/diagrams/there.svg"]),
    ...overrides,
  };
}

const rewrite = (markdown: string, overrides = {}) =>
  rewriteLinks(markdown, context(overrides));

describe("a link that resolves", () => {
  it("sends a research document to the repository, because the site has no route for one", () => {
    expect(rewrite("see [that](../docs/there.md).")).toBe(
      `see [that](${site.repo}/blob/main/docs/there.md).`,
    );
  });

  it("keeps an anchor on the way out, and checks it against the document's own headings", () => {
    expect(rewrite("see [that](../docs/there.md#a-heading).")).toBe(
      `see [that](${site.repo}/blob/main/docs/there.md#a-heading).`,
    );
  });

  it("prefixes a site route written directly, which is how a guide page links", () => {
    expect(rewrite("see [it](/guide/what-this-is).")).toBe(
      `see [it](${PREFIX}/guide/what-this-is).`,
    );
  });

  it("sends a source file to the repository without pretending to know its headings", () => {
    expect(rewrite("see [it](../crates/subtrackt-core/src/glyph.rs).")).toBe(
      `see [it](${site.repo}/blob/main/crates/subtrackt-core/src/glyph.rs).`,
    );
  });

  it("prefixes an embedded diagram, which is a public/ file rather than a route", () => {
    expect(rewrite("![flow](/diagrams/there.svg)")).toBe(
      `![flow](${PREFIX}/diagrams/there.svg)`,
    );
  });

  it("leaves an absolute URL alone", () => {
    const line = "see [#98](https://github.com/x/y/issues/98).";
    expect(rewrite(line)).toBe(line);
  });

  it("rewrites a reference definition, which is how the issue links are written", () => {
    expect(rewrite("[spec]: ../docs/there.md#a-heading")).toBe(
      `[spec]: ${site.repo}/blob/main/docs/there.md#a-heading`,
    );
  });

  it("leaves an in-page anchor as it is, having checked it exists", () => {
    expect(rewrite("see [above](#own-heading).")).toBe("see [above](#own-heading).");
  });
});

describe("a link that does not resolve", () => {
  it("is a failure rather than a rewritten guess when the target is a bare filename", () => {
    expect(() => rewrite("[x](nowhere.md)")).toThrowError(/cannot rewrite link target/);
  });

  // Leaving the site is not leaving the check. These documents are read on GitHub, where a
  // renamed heading breaks the link just as quietly as it would have here.
  it("is a failure when a research document is named that docs/ does not have", () => {
    expect(() => rewrite("[x](../docs/nowhere.md)")).toThrowError(
      /no such document in docs/,
    );
  });

  it("is a failure when the research document exists and the anchor does not", () => {
    expect(() => rewrite("[x](../docs/there.md#no-such-heading)")).toThrowError(
      /no heading with that anchor/,
    );
  });

  it("is a failure when an in-page anchor names a heading that was renamed", () => {
    expect(() => rewrite("[x](#renamed-away)")).toThrowError(/no heading with that anchor/);
  });

  // The point of checking this at all: a missing image is not a build error anywhere else. It
  // renders as a broken box on a page that compiled, which is the failure mode this file exists
  // to prevent, and the one nobody notices until a reader reports it.
  it("is a failure when a page embeds an image that is not in public/", () => {
    expect(() => rewrite("![x](/diagrams/gone.svg)")).toThrowError(/not in public/);
  });

  it("is a failure when a site route names a page that is not in site.json", () => {
    expect(() => rewrite("[x](/guide/invented)")).toThrowError(/there is no such page/);
  });

  it("names the file it came from, since the failure is read in CI output", () => {
    expect(() => rewrite("[x](nowhere.md)")).toThrowError(/content\/guide\/here\.md/);
  });
});

describe("code is not prose", () => {
  it("leaves a link-shaped thing inside a fenced block alone", () => {
    const block = "```\n$ cp [a](nowhere.md) out\n```";
    expect(rewrite(block)).toBe(block);
  });

  it("leaves a link-shaped thing inside a code span alone", () => {
    expect(rewrite("run `[a](nowhere.md)` now")).toBe("run `[a](nowhere.md)` now");
  });

  it("does not mistake a number between spaces for its own placeholder", () => {
    // The first attempt at masking used ` 12 ` as the sentinel, and these documents are full of
    // numbers between spaces. Every one of them came back as `undefined`.
    expect(rewrite("`x` reads 500 cues in 22 seconds, `y` does not")).toBe(
      "`x` reads 500 cues in 22 seconds, `y` does not",
    );
  });
});

describe("fenced languages", () => {
  it("renames console, which GitHub knows and the highlighter does not", () => {
    expect(rewriteFences("```console\n$ ls\n```")).toBe("```bash\n$ ls\n```");
  });

  it("leaves a language the highlighter already has", () => {
    expect(rewriteFences("```rust\nfn main() {}\n```")).toBe("```rust\nfn main() {}\n```");
  });

  it("leaves an untagged fence untagged", () => {
    expect(rewriteFences("```\nplain\n```")).toBe("```\nplain\n```");
  });
});

describe("heading anchors", () => {
  it("are slugged the way the site slugs them, through the markdown rather than the source", () => {
    const { anchors } = readHeadings("## Table 1 — **Accuracy** over the `sample`");
    expect(anchors).toContain("table-1--accuracy-over-the-sample");
  });

  it("carry the duplicate counter, so two identical headings do not collide", () => {
    const { anchors } = readHeadings("## What it costs\n\n## What it costs");
    expect([...anchors]).toEqual(["what-it-costs", "what-it-costs-1"]);
  });
});

describe("the generated site", () => {
  it("has a page for every entry in site.json and nothing else", () => {
    const expected = [
      ...site.guide.map((slug: string) => `/guide/${slug}`),
      ...site.usage.map((slug: string) => `/usage/${slug}`),
    ];
    expect(pages.map((page) => page.route).sort()).toEqual(expected.sort());
  });

  it("publishes no research document, which is the point of them living in the repository", () => {
    expect(pages.map((page) => page.collection)).not.toContain("research");
  });

  // A `.md` in an href is fine now, and is in fact what a link into the repository looks like --
  // so the check is that nothing was left *relative*, which is the form that 404s here.
  it("leaves no link written the way GitHub needs it", () => {
    for (const page of pages) {
      expect(page.markdown, page.source).not.toMatch(/]\((?!https?:)[^)]*\.md[)#]/);
      expect(page.markdown, page.source).not.toMatch(/]\(\.\.\//);
    }
  });

  // Same argument as the corpus check: run over what is really on disk, so a diagram deleted or
  // renamed fails on the pull request that did it rather than on the deploy after it.
  it("embeds only diagrams that are really in public/", async () => {
    const assets = await readAssets();
    const embedded = pages.flatMap((page) => [
      ...page.markdown.matchAll(/!\[[^\]]*\]\(([^)]+)\)/g),
    ].map((m) => ({ source: page.source, target: m[1] })));
    for (const { source, target } of embedded) {
      expect(target, source).toMatch(new RegExp(`^${PREFIX}/`));
      expect(assets, source).toContain(target.slice(PREFIX.length));
    }
  });

  it("leaves no fence the highlighter would reject", () => {
    for (const page of pages) {
      expect(page.markdown, page.source).not.toMatch(/^\s*(?:`{3,}|~{3,})console/m);
    }
  });

  it("gives every page the frontmatter Prestige reads", () => {
    for (const page of pages) {
      expect(page.markdown, page.source).toMatch(/^---\n(?:.*\n)*?---\n/);
      for (const key of ["title", "description", "label"]) {
        expect(page.markdown.split("\n---")[0], `${page.source} ${key}`).toMatch(
          new RegExp(`^${key}: .+$`, "m"),
        );
      }
    }
  });

});

// This one cannot be tested through `buildPages()` over the real corpus the way the rest are, and
// the reason is the reason the check exists. An unparseable frontmatter block takes gray-matter
// down inside the Vite plugin while vitest is still loading its config, so a test asserting over
// the built pages never runs: the suite is already a startup error naming `js-yaml`. The generator
// refuses first, before the build and before the tests, which is why these call it directly.
describe("frontmatter a page cannot be built without", () => {
  const page = (matter: string) => `---\n${matter}\n---\n\n# Title\n`;

  it("passes the ordinary case", () => {
    expect(() => checkFrontmatter("x.md", page("title: A\nlabel: A\ndescription: B"))).not.toThrow();
  });

  it("refuses a value carrying a colon, which YAML reads as a mapping and not as prose", () => {
    expect(() =>
      checkFrontmatter("x.md", page("description: A note: what this is")),
    ).toThrowError(/YAML reads/);
  });

  it("names the page, because the failure it replaces named js-yaml", () => {
    expect(() =>
      checkFrontmatter("content/guide/x.md", page("description: A note: b")),
    ).toThrowError(/content\/guide\/x\.md/);
  });

  it("allows a colon in a value that is quoted, which is the other way to write one", () => {
    expect(() =>
      checkFrontmatter("x.md", page('description: "A note: what this is"')),
    ).not.toThrow();
  });

  it("refuses a page with no frontmatter at all", () => {
    expect(() => checkFrontmatter("x.md", "# Title\n")).toThrowError(/no frontmatter/);
  });
});

// The corpus is not built any more, so nothing would notice a sibling link that stopped resolving.
// This is what notices. It runs over the real `docs/`, so a document renamed without its referrers
// being updated fails on the pull request that renamed it.
describe("the research corpus, which the site links out to", () => {
  const repo = new Map([
    ["docs/here.md", new Set(["own-heading"])],
    ["docs/there.md", new Set(["a-heading"])],
  ]);
  const check = (markdown: string) => checkCorpus("docs/here.md", markdown, repo);

  it("passes a sibling link, which is how these documents cross-reference on GitHub", () => {
    expect(() => check("see [that](there.md#a-heading).")).not.toThrow();
  });

  it("fails a sibling that does not exist", () => {
    expect(() => check("see [that](gone.md).")).toThrowError(/no such document in docs/);
  });

  it("fails an anchor no heading in the sibling answers", () => {
    expect(() => check("see [that](there.md#moved).")).toThrowError(
      /no heading with that anchor/,
    );
  });

  it("fails an in-page anchor that names a heading which was renamed", () => {
    expect(() => check("see [above](#renamed).")).toThrowError(
      /no heading with that anchor/,
    );
  });

  it("leaves a path out of docs/ alone, having no business asserting what is in it", () => {
    expect(() => check("see [it](../crates/subtrackt-core/src/glyph.rs).")).not.toThrow();
  });

  it("leaves a link inside a fenced block alone, because that is an example", () => {
    expect(() => check("```\n$ cp [a](gone.md) out\n```")).not.toThrow();
  });

  it("runs over the real corpus, which is the only reason any of this catches anything", () => {
    // `buildPages()` above already ran it. If `docs/` had a broken cross-link, the top of this
    // file would have thrown before a single test was collected.
    expect(pages.length).toBeGreaterThan(0);
  });
});

describe("the hand-written half", () => {
  const guide = pages.filter((page) => page.collection === "guide");
  const usage = pages.filter((page) => page.collection === "usage");

  it("quotes no character error rate, which is the research half's job", () => {
    for (const page of guide) {
      expect(page.markdown, page.source).not.toMatch(/\bCER\b/);
      expect(page.markdown, page.source).not.toMatch(/character error rate/i);
    }
  });

  it("cites no issue number, because a reader here has not read the issues", () => {
    for (const page of guide) {
      expect(page.markdown, page.source).not.toMatch(/(?:^|\s)#\d+\b/);
    }
  });

  it("cites no issue number from the usage half either, for the same reason", () => {
    for (const page of usage) {
      expect(page.markdown, page.source).not.toMatch(/(?:^|\s)#\d+\b/);
    }
  });

  it("reaches the research corpus, so a reader who wants the workings can get to them", () => {
    const linked = [...guide, ...usage].flatMap((page) => [
      ...page.markdown.matchAll(
        new RegExp(`${site.repo}/blob/main/docs/([a-z-]+)\\.md`, "g"),
      ),
    ]);
    expect(new Set(linked.map((m) => m[1])).size).toBeGreaterThan(3);
  });

  // A diagram is the one thing on this site a screen reader cannot fall back to the prose for, so
  // the alt text is not decoration: it is the page for a reader who cannot see the picture. Empty
  // alt text on a diagram carrying the argument would be a silent hole in the page.
  it("describes every diagram it embeds, at length rather than in a word", () => {
    for (const page of pages) {
      for (const [, alt] of page.markdown.matchAll(/!\[([^\]]*)\]\([^)]+\)/g)) {
        expect(alt.length, `${page.source}: "${alt}"`).toBeGreaterThan(40);
      }
    }
  });

  // The comparison is the measurement behind the claim the whole tool rests on, and it spent its
  // life three sidebars away in a section that no longer exists. It is a guide page now.
  it("carries the tool comparison in the half a reader actually reads", () => {
    expect(site.guide).toContain("how-it-compares");
    const page = guide.find((p) => p.slug === "how-it-compares")!;
    expect(page.markdown).toMatch(/PgsToSrt/);
    expect(page.markdown).toMatch(/pgsrip/);
    expect(page.markdown).toMatch(new RegExp(`${site.repo}/blob/main/docs/alternatives\\.md`));
  });

  // The usage half is a command reference, so the thing it must not do is silently stop covering a
  // command. `--help` is the only other place the list exists, and a subcommand added there with no
  // page here is exactly the drift a reader would find by hitting a gap.
  it("has a page for every subcommand the binary offers", () => {
    for (const command of ["list", "extract", "fit", "glyphs", "gen-reference"]) {
      expect(
        usage.map((page) => page.slug),
        `no usage page documents \`subtrackt ${command}\``,
      ).toContain(command);
    }
  });

  it("opens the usage half with a quick start, which is what the root links at", () => {
    expect(site.usage[0]).toBe("quick-start");
  });
});
