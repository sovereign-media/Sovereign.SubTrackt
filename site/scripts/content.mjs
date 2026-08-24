// Generates `src/content/` -- every page the site serves -- from two sources that stay canonical
// where they are.
//
// `docs/*.md`, at the repository root, is the research corpus. The README links into it twenty-odd
// times, some with anchors, and every one of those URLs is one somebody may already have; moving
// the documents here would break all of them. `content/guide/*.md`, beside this script, is the
// hand-written plain-language half, and it is committed.
//
// Both go through the same rewrite, and everything under `src/content/` is a build artefact:
// gitignored, regenerated on every build, never edited.
//
// Two things have to be rewritten, and neither can be done as a Prestige markdown plugin --
// `prestige.config.ts` accepts a `markdown.rehypePlugins` list, documents it in its own reference,
// and never passes it to the compiler. Version 0.15.0, checked.
//
//   Links. A research document links to its siblings as `reference-set.md`, which is right on
//   GitHub and a 404 on a site whose route is `/research/reference-set`. Site-absolute links then
//   need the `/Sovereign.SubTrackt/` prefix, because nothing else in the build puts it on an href
//   that came out of a document -- Vite's `base` reaches assets and the router's basepath reaches
//   its own `Link`s, and neither touches this. Doing it here rather than in each document is what
//   lets both halves be written with plain `/research/reference-set`.
//
//   Fenced code languages. Every shell transcript in this repository is tagged ```console, which
//   GitHub understands and Prism does not -- and Prestige treats an unregistered language as a
//   hard compile error rather than as plain text.
//
// An unrecognised link target, a link to a page that does not exist and a `#fragment` that no
// heading answers are all build failures rather than rewritten guesses. A broken cross-link is how
// a docs site rots quietly, and this is the thing that notices.

import GithubSlugger from "github-slugger";
import { toString as mdToString } from "mdast-util-to-string";
import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import remarkGfm from "remark-gfm";
import remarkParse from "remark-parse";
import { unified } from "unified";
import { visit } from "unist-util-visit";

const SITE_DIR = dirname(dirname(fileURLToPath(import.meta.url)));
export const DOCS_DIR = join(SITE_DIR, "..", "docs");
export const GUIDE_DIR = join(SITE_DIR, "content", "guide");
export const OUT_DIR = join(SITE_DIR, "src", "content");

export const site = JSON.parse(
  await readFile(join(SITE_DIR, "site.json"), "utf8"),
);

const PREFIX = site.base.replace(/\/$/, "");

/** Every research document in sidebar order, flattened out of its group. */
export function manifestDocuments() {
  return site.research.flatMap((group) =>
    group.documents.map((doc) => ({ ...doc, group: group.label })),
  );
}

const parser = unified().use(remarkParse).use(remarkGfm);

/**
 * The heading anchors `rehype-slug` will mint for a document, and its `# ` title.
 *
 * Slugs are computed the way the site computes them -- one `GithubSlugger` per document, so the
 * duplicate-heading counter matches -- rather than by a regex over the source, because a heading
 * carrying bold or code is not its own source text.
 */
export function readHeadings(markdown) {
  const slugger = new GithubSlugger();
  const anchors = new Set();
  let title = null;
  visit(parser.parse(markdown), "heading", (node) => {
    const text = mdToString(node);
    anchors.add(slugger.slug(text));
    if (node.depth === 1 && title === null) title = text;
  });
  return { title, anchors };
}

// GitHub's name for a shell transcript on the left, Prism's on the right.
const FENCE_ALIASES = { console: "bash" };
const FENCE_OPENING = /^([ \t]*)(`{3,}|~{3,})([A-Za-z][\w-]*)/gm;

export function rewriteFences(markdown) {
  return markdown.replace(FENCE_OPENING, (match, indent, ticks, language) =>
    language in FENCE_ALIASES
      ? `${indent}${ticks}${FENCE_ALIASES[language]}`
      : match,
  );
}

// Fenced blocks and code spans are masked before the link rewrite: a shell transcript is allowed to
// contain something shaped like a link target, and rewriting inside one would corrupt an example.
const FENCED_BLOCK = /^([ \t]*)(`{3,}|~{3,})[\s\S]*?^\1?\2[ \t]*$/gm;
const CODE_SPAN = /(`+)(?:[^\n]|\n(?!\n))*?\1/g;
// NUL delimits the placeholder because prose cannot contain one. A readable sentinel cannot be
// used here: the first attempt was ` 12 `, and these documents are full of numbers between spaces.
const PLACEHOLDER = /\0(\d+)\0/g;

function maskCode(text) {
  const held = [];
  const hold = (match) => `\0${held.push(match) - 1}\0`;
  const masked = text.replace(FENCED_BLOCK, hold).replace(CODE_SPAN, hold);
  return {
    masked,
    unmask: (s) => s.replace(PLACEHOLDER, (_, i) => held[Number(i)]),
  };
}

const INLINE_LINK =
  /(!?\[(?:[^[\]\\]|\\.)*\])\(\s*(<[^>]*>|[^\s()]+)((?:\s+(?:"[^"]*"|'[^']*'))?)\s*\)/g;
const REFERENCE_DEFINITION = /^(\[[^\]\n]+\]:[ \t]*)(\S+)/gm;

/**
 * Rewrite one link target from what it means in its source directory to what it means on the site.
 *
 * `source` names the page being rewritten, for error messages and for its own `#fragment`s.
 * `routes` maps every site route to its heading anchors. `research` is the set of research
 * filenames, empty for a guide page -- `reference-set.md` means a sibling document only inside the
 * research corpus, and a guide page writing that has made a mistake worth reporting.
 */
export function rewriteTarget(target, { source, route, routes, research }) {
  const bare = target.replace(/^<|>$/g, "");

  // Absolute URLs already mean the same thing wherever they are read.
  if (/^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(bare)) return target;

  // An in-page anchor needs no rewrite, but is still checked: a renamed heading breaks one
  // silently, and silently is the whole problem.
  if (bare.startsWith("#")) {
    requireAnchor(source, bare.slice(1), routes.get(route), route);
    return target;
  }

  const [path, ...rest] = bare.split("#");
  const fragment = rest.join("#");
  const suffix = fragment ? `#${fragment}` : "";

  // A sibling research document, written the way GitHub needs it.
  if (research.has(path)) {
    return withPrefix(source, `/research/${path.replace(/\.md$/, "")}`, fragment);
  }

  // A site route, written the way both halves are told to write one.
  if (path.startsWith("/")) return withPrefix(source, path, fragment);

  // Anything outside the two content directories -- source files, the hand-verified transcripts --
  // has no site route and leaves for GitHub, which is what the document's author meant anyway.
  if (path.startsWith("../")) {
    return `${site.repo}/blob/main/${path.slice(3)}${suffix}`;
  }

  throw new Error(
    `${source}: cannot rewrite link target ${JSON.stringify(target)}. ` +
      "A relative target must be a research document or a path into the repository " +
      'beginning "../"; anything else must be a site route beginning "/" or an absolute URL.',
  );

  function withPrefix(from, sitePath, frag) {
    if (!routes.has(sitePath)) {
      throw new Error(
        `${from}: link to "${sitePath}" does not resolve -- there is no such page. ` +
          "Pages come from site.json; a research document also needs a file in docs/.",
      );
    }
    if (frag) requireAnchor(from, frag, routes.get(sitePath), sitePath);
    return `${PREFIX}${sitePath}${frag ? `#${frag}` : ""}`;
  }
}

function requireAnchor(source, fragment, anchors, target) {
  if (anchors.has(fragment)) return;
  throw new Error(
    `${source}: link to "${target}#${fragment}" does not resolve -- ` +
      `${target} has no heading with that anchor.`,
  );
}

/** Rewrite every link in one document's markdown. */
export function rewriteLinks(markdown, context) {
  const { masked, unmask } = maskCode(markdown);
  const rewritten = masked
    .replace(INLINE_LINK, (_, label, target, title) => {
      return `${label}(${rewriteTarget(target, context)}${title})`;
    })
    .replace(REFERENCE_DEFINITION, (_, head, target) => {
      return `${head}${rewriteTarget(target, context)}`;
    });
  return unmask(rewritten);
}

function yaml(value) {
  // JSON's string form is a valid YAML double-quoted scalar, and it escapes the quotes and
  // backslashes a title lifted out of a document can carry.
  return JSON.stringify(value);
}

async function readSources(dir, names) {
  const sources = new Map();
  for (const name of names) {
    sources.set(name, await readFile(join(dir, name), "utf8"));
  }
  return sources;
}

/**
 * Check `docs/` and `content/guide/` against `site.json`, and return every page to write.
 *
 * Nothing is written here, so a test can call this and get the same failures the build gets.
 */
export async function buildPages() {
  const entries = manifestDocuments();
  const research = new Set(entries.map((entry) => entry.file));

  const docsOnDisk = (await readdir(DOCS_DIR))
    .filter((name) => name.endsWith(".md"))
    .sort();
  const guideOnDisk = (await readdir(GUIDE_DIR))
    .filter((name) => name.endsWith(".md"))
    .sort();
  const guideListed = site.guide.map((slug) => `${slug}.md`);

  const problems = [
    ...docsOnDisk
      .filter((n) => !research.has(n))
      .map((n) => `  docs/${n} is not listed in site.json`),
    ...entries
      .filter((e) => !docsOnDisk.includes(e.file))
      .map((e) => `  site.json lists ${e.file}, which is not in docs/`),
    ...guideOnDisk
      .filter((n) => !guideListed.includes(n))
      .map((n) => `  content/guide/${n} is not listed in site.json`),
    ...guideListed
      .filter((n) => !guideOnDisk.includes(n))
      .map((n) => `  site.json lists guide/${n}, which is not in content/guide/`),
  ];
  if (problems.length) {
    throw new Error(
      `site.json does not describe what is on disk:\n${problems.join("\n")}\n` +
        "site.json is what orders the sidebar, so every page needs an entry in it.",
    );
  }

  const docs = await readSources(DOCS_DIR, docsOnDisk);
  const guide = await readSources(GUIDE_DIR, guideListed);

  // Every route first, with its anchors, so a link can be checked against a page that has not been
  // rewritten yet -- including a forward reference from the guide into the research corpus.
  const routes = new Map();
  const headings = new Map();
  for (const [collection, sources] of [
    ["research", docs],
    ["guide", guide],
  ]) {
    for (const [name, markdown] of sources) {
      const parsed = readHeadings(markdown);
      const route = `/${collection}/${name.replace(/\.md$/, "")}`;
      headings.set(`${collection}/${name}`, parsed);
      routes.set(route, parsed.anchors);
    }
  }

  const pages = [];

  for (const entry of entries) {
    const key = `research/${entry.file}`;
    const slug = entry.file.replace(/\.md$/, "");
    const { title } = headings.get(key);
    if (!title) {
      throw new Error(`docs/${entry.file}: no "# " heading to take a title from.`);
    }
    pages.push({
      collection: "research",
      slug,
      route: `/research/${slug}`,
      source: `docs/${entry.file}`,
      // A research document carries no frontmatter -- it is read on GitHub as often as here -- so
      // the site's own metadata is prepended. The title is the document's own `# ` heading; the
      // label and description come from site.json, which is where a group is chosen anyway.
      markdown: withFrontmatter(
        { title, description: entry.description, label: entry.label },
        rewrite(docs.get(entry.file), `docs/${entry.file}`, `/research/${slug}`),
      ),
    });
  }

  for (const slug of site.guide) {
    const name = `${slug}.md`;
    pages.push({
      collection: "guide",
      slug,
      route: `/guide/${slug}`,
      source: `content/guide/${name}`,
      // Written by hand, so it carries its own frontmatter and nothing is prepended.
      markdown: rewrite(guide.get(name), `content/guide/${name}`, `/guide/${slug}`),
    });
  }

  return pages;

  function rewrite(markdown, source, route) {
    return rewriteFences(
      rewriteLinks(markdown, { source, route, routes, research }),
    );
  }
}

function withFrontmatter(matter, body) {
  return [
    "---",
    `title: ${yaml(matter.title)}`,
    `description: ${yaml(matter.description)}`,
    `label: ${yaml(matter.label)}`,
    "---",
    "",
    body,
  ].join("\n");
}

export async function generate() {
  const pages = await buildPages();
  await rm(OUT_DIR, { recursive: true, force: true });
  for (const collection of new Set(pages.map((p) => p.collection))) {
    await mkdir(join(OUT_DIR, collection), { recursive: true });
  }
  for (const page of pages) {
    await writeFile(
      join(OUT_DIR, page.collection, `${page.slug}.md`),
      page.markdown,
    );
  }
  return pages;
}
