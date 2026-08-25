// Generates `src/content/` -- every page the site serves -- from `content/<collection>/*.md`, and
// checks the research corpus in `docs/` that the site no longer publishes.
//
// The site is `guide/` and `usage/`, written by hand and committed beside this script. Everything
// under `src/content/` is a build artefact: gitignored, regenerated on every build, never edited.
//
// `docs/*.md` at the repository root used to be a third collection. It is read on GitHub now, and
// the pages here link out to it. It is still parsed on every build, for one reason given below.
//
// Two things have to be rewritten, and neither can be done as a Prestige markdown plugin --
// `prestige.config.ts` accepts a `markdown.rehypePlugins` list, documents it in its own reference,
// and never passes it to the compiler. Version 0.15.0, checked.
//
//   Links. A site-absolute link needs the `/Sovereign.SubTrackt/` prefix, because nothing else in
//   the build puts it on an href that came out of a document -- Vite's `base` reaches assets and
//   the router's basepath reaches its own `Link`s, and neither touches this. Doing it here lets a
//   page be written with a plain `/guide/what-this-is`. A `../` target is a path into the
//   repository and leaves for GitHub, which is how a page reaches `docs/` and the source tree.
//   An image is site-absolute too and is not a route, so it is resolved against `public/` instead
//   of against the route table -- same prefix, different thing to check it exists in.
//
//   Fenced code languages. Every shell transcript in this repository is tagged ```console, which
//   GitHub understands and Prism does not -- and Prestige treats an unregistered language as a
//   hard compile error rather than as plain text.
//
// An unrecognised link target, a link to a page that does not exist and a `#fragment` that no
// heading answers are all build failures rather than rewritten guesses. A broken cross-link is how
// a docs site rots quietly, and this is the thing that notices.
//
// That check is why `docs/` is still read here. Those documents left the site; they did not leave
// the repository, and a page that links to `docs/alternatives.md#table-4--cost` can still name a
// heading that no longer exists. So every `../docs/*.md` target is resolved against the document
// on disk and every fragment against its real headings -- and the corpus is checked against itself
// the same way, because a link between two documents nobody builds is exactly the kind that rots.

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
export const OUT_DIR = join(SITE_DIR, "src", "content");
// Vite copies this directory to the root of the built site verbatim. A page embeds a diagram from
// it as `/diagrams/pipeline.svg`, which is why an asset needs a rewrite rule of its own: it is
// site-absolute like a route and is not a route, so the route table would refuse it.
export const PUBLIC_DIR = join(SITE_DIR, "public");

// The hand-written collections. Each is a `site.json` key holding a list of slugs and a directory
// of the same name under `content/`, so adding a third one is adding it here and nowhere else in
// this file.
export const HANDWRITTEN = ["guide", "usage"];

/** Where one hand-written collection's markdown lives. */
export function handwrittenDir(collection) {
  return join(SITE_DIR, "content", collection);
}

export const site = JSON.parse(
  await readFile(join(SITE_DIR, "site.json"), "utf8"),
);

const PREFIX = site.base.replace(/\/$/, "");

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

// What an embedded asset looks like. Deliberately a closed list rather than "has an extension":
// the point of the rule below is to separate an asset from a route, and a route has no extension,
// so anything not named here stays a route and gets the route table's error message.
const ASSET = /\.(?:svg|png|jpe?g|gif|webp|avif)$/i;

/** Every file under `public/`, as the site-absolute path a page would write to embed it. */
export async function readAssets(dir = PUBLIC_DIR, prefix = "") {
  const found = new Set();
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = `${prefix}/${entry.name}`;
    if (entry.isDirectory()) {
      for (const nested of await readAssets(join(dir, entry.name), path)) found.add(nested);
    } else found.add(path);
  }
  return found;
}

/**
 * Rewrite one link target from what it means in its source directory to what it means on the site.
 *
 * `source` names the page being rewritten, for error messages and for its own `#fragment`s.
 * `routes` maps every site route to its heading anchors. `repo` maps every repository path that
 * has checkable headings -- `docs/alternatives.md` and the like -- to that document's anchors, so
 * a link leaving for GitHub is still held to the same standard as one that stays. `assets` is what
 * `public/` holds, for the diagrams a page embeds.
 */
export function rewriteTarget(target, { source, route, routes, repo, assets }) {
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

  // A diagram, embedded from `public/`. Checked against the directory rather than against the
  // route table, and checked at all for the reason every other target here is: a missing image
  // renders as a broken box on a page that built cleanly, which is the quiet rot this file exists
  // to prevent. It needs the same base prefix a route does -- nothing else in the build puts one
  // on an href that came out of a document.
  if (path.startsWith("/") && ASSET.test(path)) {
    if (!assets.has(path)) {
      throw new Error(
        `${source}: embeds "${path}", which is not in public/. ` +
          "An image is served from public/ and referenced by its path from the site root.",
      );
    }
    return `${PREFIX}${path}`;
  }

  // A site route, written the way both halves are told to write one.
  if (path.startsWith("/")) return withPrefix(source, path, fragment);

  // A path into the repository -- a research document, a source file, a hand-verified transcript.
  // None has a site route, and GitHub is where the author meant it to land. A document whose
  // headings this build can read is checked against them anyway; see the note at the top.
  if (path.startsWith("../")) {
    const inRepo = path.slice(3);
    if (fragment && repo.has(inRepo)) {
      requireAnchor(source, fragment, repo.get(inRepo), inRepo);
    }
    if (inRepo.endsWith(".md") && inRepo.startsWith("docs/") && !repo.has(inRepo)) {
      throw new Error(
        `${source}: link to "${path}" does not resolve -- there is no such document in docs/.`,
      );
    }
    return `${site.repo}/blob/main/${inRepo}${suffix}`;
  }

  throw new Error(
    `${source}: cannot rewrite link target ${JSON.stringify(target)}. ` +
      'A relative target must be a path into the repository beginning "../"; ' +
      'anything else must be a site route beginning "/" or an absolute URL.',
  );

  function withPrefix(from, sitePath, frag) {
    if (!routes.has(sitePath)) {
      throw new Error(
        `${from}: link to "${sitePath}" does not resolve -- there is no such page. ` +
          "Pages come from site.json. A document in docs/ is not a page; link to it as ../docs/x.md.",
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
 * Check `content/` against `site.json`, check `docs/` against itself, and return every page.
 *
 * Nothing is written here, so a test can call this and get the same failures the build gets.
 */
export async function buildPages() {
  const docsOnDisk = (await readdir(DOCS_DIR))
    .filter((name) => name.endsWith(".md"))
    .sort();

  const listed = new Map(
    HANDWRITTEN.map((id) => [id, site[id].map((slug) => `${slug}.md`)]),
  );
  const onDisk = new Map();
  for (const id of HANDWRITTEN) {
    onDisk.set(
      id,
      (await readdir(handwrittenDir(id)))
        .filter((name) => name.endsWith(".md"))
        .sort(),
    );
  }

  const problems = HANDWRITTEN.flatMap((id) => [
    ...onDisk
      .get(id)
      .filter((n) => !listed.get(id).includes(n))
      .map((n) => `  content/${id}/${n} is not listed in site.json`),
    ...listed
      .get(id)
      .filter((n) => !onDisk.get(id).includes(n))
      .map((n) => `  site.json lists ${id}/${n}, which is not in content/${id}/`),
  ]);
  if (problems.length) {
    throw new Error(
      `site.json does not describe what is on disk:\n${problems.join("\n")}\n` +
        "site.json is what orders the sidebar, so every page needs an entry in it.",
    );
  }

  const docs = await readSources(DOCS_DIR, docsOnDisk);
  const assets = await readAssets();
  const handwritten = new Map();
  for (const id of HANDWRITTEN) {
    handwritten.set(id, await readSources(handwrittenDir(id), listed.get(id)));
  }

  // Every route first, with its anchors, so a link can be checked against a page that has not been
  // rewritten yet -- which is most of them, since the usage half cross-references in both
  // directions and the guide links forward into pages that come after it.
  const routes = new Map();
  for (const [collection, sources] of handwritten) {
    for (const [name, markdown] of sources) {
      const route = `/${collection}/${name.replace(/\.md$/, "")}`;
      routes.set(route, readHeadings(markdown).anchors);
    }
  }

  // The repository paths whose headings this build can read. Nothing here becomes a page; it is
  // what lets a `../docs/alternatives.md#table-4--cost` be held to the same standard as a link
  // that stays on the site.
  const repo = new Map();
  for (const [name, markdown] of docs) {
    repo.set(`docs/${name}`, readHeadings(markdown).anchors);
  }
  for (const [name, markdown] of docs) {
    checkCorpus(`docs/${name}`, markdown, repo);
  }

  const pages = [];
  for (const collection of HANDWRITTEN) {
    for (const slug of site[collection]) {
      const name = `${slug}.md`;
      const source = `content/${collection}/${name}`;
      const markdown = handwritten.get(collection).get(name);
      checkFrontmatter(source, markdown);
      pages.push({
        collection,
        slug,
        route: `/${collection}/${slug}`,
        source,
        // Written by hand, so it carries its own frontmatter and nothing is prepended.
        markdown: rewriteFences(
          rewriteLinks(markdown, {
            source,
            route: `/${collection}/${slug}`,
            routes,
            repo,
            assets,
          }),
        ),
      });
    }
  }

  return pages;
}

/**
 * Refuse a hand-written page whose frontmatter YAML would not parse.
 *
 * One case, and it is the one that keeps happening: `description: A note: what this is`. A `: ` in
 * an unquoted scalar makes it a mapping, and the value has no key. What that costs is out of all
 * proportion to the typo -- gray-matter throws inside the Vite plugin while vitest is still loading
 * its config, so the whole suite reports as a startup error, every test in it is marked failed, and
 * the stack names `js-yaml` rather than the page. Refused here, where the message can name the file
 * and say what to do, and where it happens before either the build or the tests get going.
 */
export function checkFrontmatter(source, markdown) {
  const matter = markdown.match(/^---\n([\s\S]*?)\n---\n/);
  if (!matter) {
    throw new Error(`${source}: no frontmatter block. Prestige reads title, label, description.`);
  }
  for (const line of matter[1].split("\n")) {
    const value = line.match(/^[A-Za-z][\w-]*: (.*)$/)?.[1];
    if (value === undefined || /^["']/.test(value)) continue;
    if (value.includes(": ")) {
      throw new Error(
        `${source}: frontmatter value ${JSON.stringify(value)} contains ": ", which YAML reads ` +
          "as a mapping rather than as a colon. Quote the value, or reword without the colon.",
      );
    }
  }
}

/**
 * Check one research document's links the way GitHub resolves them.
 *
 * These documents are not pages any more, so nothing rewrites them and nothing would otherwise
 * notice a sibling that was renamed or a heading that moved. They are read where they sit, so
 * `reference-set.md` means the file beside it and `#table-4--cost` means a real heading, and both
 * are checkable from here for the cost of a parse.
 *
 * A `../` target leaves `docs/` for the source tree and is left alone: those are paths to Rust
 * files and transcripts, and this build has no business asserting what is in them.
 */
export function checkCorpus(source, markdown, repo) {
  const self = repo.get(source);
  const { masked } = maskCode(markdown);
  const targets = [
    ...[...masked.matchAll(INLINE_LINK)].map((m) => m[2]),
    ...[...masked.matchAll(REFERENCE_DEFINITION)].map((m) => m[2]),
  ];

  for (const raw of targets) {
    const bare = raw.replace(/^<|>$/g, "");
    if (/^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(bare)) continue;
    if (bare.startsWith("../")) continue;

    if (bare.startsWith("#")) {
      requireAnchor(source, bare.slice(1), self, source);
      continue;
    }

    const [path, ...rest] = bare.split("#");
    const fragment = rest.join("#");
    const sibling = `docs/${path}`;
    if (!repo.has(sibling)) {
      throw new Error(
        `${source}: link to "${path}" does not resolve -- there is no such document in docs/. ` +
          "A research document links to a sibling by filename, the way GitHub reads it.",
      );
    }
    if (fragment) requireAnchor(source, fragment, repo.get(sibling), path);
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
