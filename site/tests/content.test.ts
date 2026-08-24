// The link rewrite is the only real logic in this site, and a broken cross-link is how a docs site
// rots quietly, so it is the thing with tests.
//
// `buildPages()` is run against the real corpus rather than against fixtures. That is deliberate:
// it means adding a document to `docs/` that links to a heading which does not exist fails here,
// on a pull request, rather than shipping. `scripts/check-links.mjs` then follows the same links
// through the built HTML, which is what catches a page that was never rendered at all.

import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";
import {
  buildPages,
  readHeadings,
  rewriteFences,
  rewriteLinks,
  site,
} from "../scripts/content.mjs";

const PREFIX = site.base.replace(/\/$/, "");
const pages = await buildPages();

/** A rewrite context standing in for one research document that links to one sibling. */
function context(overrides: Record<string, unknown> = {}) {
  return {
    source: "docs/here.md",
    route: "/research/here",
    routes: new Map([
      ["/research/here", new Set(["own-heading"])],
      ["/research/there", new Set(["a-heading"])],
      ["/guide/what-this-is", new Set<string>()],
    ]),
    research: new Set(["here.md", "there.md"]),
    ...overrides,
  };
}

const rewrite = (markdown: string, overrides = {}) =>
  rewriteLinks(markdown, context(overrides));

describe("a link that resolves", () => {
  it("turns a sibling document into a site route carrying the base path", () => {
    expect(rewrite("see [that](there.md).")).toBe(
      `see [that](${PREFIX}/research/there).`,
    );
  });

  it("keeps an anchor, and checks it against the target's own headings", () => {
    expect(rewrite("see [that](there.md#a-heading).")).toBe(
      `see [that](${PREFIX}/research/there#a-heading).`,
    );
  });

  it("prefixes a site route written directly, which is how a guide page links", () => {
    expect(rewrite("see [it](/guide/what-this-is).")).toBe(
      `see [it](${PREFIX}/guide/what-this-is).`,
    );
  });

  it("sends a path out of docs/ to the repository, because the site has no route for one", () => {
    expect(rewrite("see [it](../crates/subtrackt-core/src/glyph.rs).")).toBe(
      `see [it](${site.repo}/blob/main/crates/subtrackt-core/src/glyph.rs).`,
    );
  });

  it("leaves an absolute URL alone", () => {
    const line = "see [#98](https://github.com/x/y/issues/98).";
    expect(rewrite(line)).toBe(line);
  });

  it("rewrites a reference definition, which is how the issue links are written", () => {
    expect(rewrite("[spec]: there.md#a-heading")).toBe(
      `[spec]: ${PREFIX}/research/there#a-heading`,
    );
  });

  it("leaves an in-page anchor as it is, having checked it exists", () => {
    expect(rewrite("see [above](#own-heading).")).toBe("see [above](#own-heading).");
  });
});

describe("a link that does not resolve", () => {
  it("is a failure rather than a rewritten guess when the document is unknown", () => {
    expect(() => rewrite("[x](nowhere.md)")).toThrowError(/cannot rewrite link target/);
  });

  it("is a failure when the page exists and the anchor does not", () => {
    expect(() => rewrite("[x](there.md#no-such-heading)")).toThrowError(
      /there.md#no-such-heading|\/research\/there#no-such-heading/,
    );
  });

  it("is a failure when an in-page anchor names a heading that was renamed", () => {
    expect(() => rewrite("[x](#renamed-away)")).toThrowError(/no heading with that anchor/);
  });

  it("is a failure when a site route names a page that is not in site.json", () => {
    expect(() => rewrite("[x](/guide/invented)")).toThrowError(/there is no such page/);
  });

  it("names the file it came from, since the failure is read in CI output", () => {
    expect(() => rewrite("[x](nowhere.md)")).toThrowError(/docs\/here\.md/);
  });
});

describe("code is not prose", () => {
  it("leaves a link-shaped thing inside a fenced block alone", () => {
    const block = "```\n$ cp [a](there.md) out\n```";
    expect(rewrite(block)).toBe(block);
  });

  it("leaves a link-shaped thing inside a code span alone", () => {
    expect(rewrite("run `[a](there.md)` now")).toBe("run `[a](there.md)` now");
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
      ...site.research.flatMap((group: { documents: { file: string }[] }) =>
        group.documents.map((doc) => `/research/${doc.file.replace(/\.md$/, "")}`),
      ),
      ...site.guide.map((slug: string) => `/guide/${slug}`),
    ];
    expect(pages.map((page) => page.route).sort()).toEqual(expected.sort());
  });

  it("leaves no link written the way GitHub needs it", () => {
    for (const page of pages) {
      expect(page.markdown, page.source).not.toMatch(/]\([^)]*\.md[)#]/);
      expect(page.markdown, page.source).not.toMatch(/]\(\.\.\//);
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

  it("changes nothing in a research document except its links and its fences", async () => {
    for (const page of pages.filter((p) => p.collection === "research")) {
      const source = await readFile(`../docs/${page.slug}.md`, "utf8");
      const body = page.markdown.split("\n---\n").slice(1).join("\n---\n").slice(1);
      const strip = (text: string) =>
        text
          .replace(/]\([^)]*\)/g, "]()")
          .replace(/^(\[[^\]\n]+\]:).*$/gm, "$1")
          .replace(/^(\s*)(`{3,}|~{3,})[A-Za-z][\w-]*/gm, "$1$2");
      expect(strip(body), page.source).toBe(strip(source));
    }
  });
});

describe("the plain-language half", () => {
  const guide = pages.filter((page) => page.collection === "guide");

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

  it("reaches the research half, so the two sections are one site", () => {
    const linked = guide.flatMap((page) => [
      ...page.markdown.matchAll(new RegExp(`${PREFIX}/research/([a-z-]+)`, "g")),
    ]);
    expect(new Set(linked.map((m) => m[1])).size).toBeGreaterThan(3);
  });
});
