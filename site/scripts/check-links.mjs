#!/usr/bin/env node
// Walks the prerendered site and follows every internal link, including the fragment.
//
// `scripts/content.mjs` already refuses to rewrite a link it cannot resolve, and that catches the
// common case. This checks the other half: that the page the link resolves to was actually
// rendered, and that the heading the fragment names came out with that id. It reads the built HTML
// rather than the markdown, so it also covers the links the framework generates -- the two sidebars,
// the previous/next pair at the foot of a page, the header -- which no check over the source could
// see. A prerender that quietly missed a page fails here.

import { readdir, readFile } from "node:fs/promises";
import { dirname, join, posix, relative } from "node:path";
import { fileURLToPath } from "node:url";

const SITE_DIR = dirname(dirname(fileURLToPath(import.meta.url)));
const DIST = join(SITE_DIR, "dist", "client");
const site = JSON.parse(await readFile(join(SITE_DIR, "site.json"), "utf8"));
const PREFIX = site.base.replace(/\/$/, "");

const HREF = /href="([^"]*)"/g;
const ID = /\bid="([^"]*)"/g;
// Images only. Every other `src` in the output is a bundler-generated script whose path this
// script has no business asserting, and an image is the one a reader notices breaking.
const SRC = /\bsrc="([^"]*)"/g;
const IMAGE = /\.(?:svg|png|jpe?g|gif|webp|avif)$/i;

async function walk(dir) {
  const found = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) found.push(...(await walk(full)));
    else found.push(full);
  }
  return found;
}

function unescapeAttribute(value) {
  return value
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#x27;/g, "'");
}

function idsIn(html) {
  return new Set([...html.matchAll(ID)].map((m) => unescapeAttribute(m[1])));
}

const files = await walk(DIST);
const pages = files.filter((f) => f.endsWith(".html"));
const present = new Set(files.map((f) => relative(DIST, f).replaceAll("\\", "/")));
const ids = new Map();
const html = new Map();
for (const file of files) {
  const route = relative(DIST, file).replaceAll("\\", "/");
  if (!file.endsWith(".html")) continue;
  const text = await readFile(file, "utf8");
  html.set(route, text);
  ids.set(route, idsIn(text));
}

// A site path as written in an href, resolved to the file GitHub Pages would serve for it.
function fileFor(path) {
  const clean = path.replace(/^\/+/, "");
  if (clean === "" || clean.endsWith("/")) return posix.join(clean, "index.html");
  if (present.has(clean)) return clean;
  const asDirectory = posix.join(clean, "index.html");
  return present.has(asDirectory) ? asDirectory : clean;
}

const failures = [];
let checked = 0;

// The diagrams. `scripts/content.mjs` already refused a page embedding an image that is not in
// `public/`; this is the other half, that Vite actually copied it out. A broken image renders as
// a box on a page that built green, which is the same quiet rot the href walk exists to catch.
//
// The href walk below happens to catch a missing diagram too, and that is a coincidence worth not
// relying on: React emits a `<link rel="preload" as="image" href="...">` for it in the head, so
// the failure is reported by a browser hint rather than by anything about the page. Drop the hint
// and the image goes unchecked. This loop reads the `<img>` itself.
for (const page of pages) {
  const from = relative(DIST, page).replaceAll("\\", "/");
  for (const [, raw] of html.get(from).matchAll(SRC)) {
    const src = unescapeAttribute(raw);
    if (!IMAGE.test(src) || /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(src)) continue;
    if (!src.startsWith(`${PREFIX}/`)) {
      failures.push(`${from}: image "${src}" is not site-absolute under the ${PREFIX} base`);
      continue;
    }
    const target = src.slice(PREFIX.length).replace(/^\/+/, "");
    if (!present.has(target)) {
      failures.push(`${from}: image "${src}" points at nothing -- no ${target} was written`);
      continue;
    }
    checked += 1;
  }
}

for (const page of pages) {
  const from = relative(DIST, page).replaceAll("\\", "/");
  for (const [, raw] of html.get(from).matchAll(HREF)) {
    const href = unescapeAttribute(raw);
    if (href === "" || /^(?:[a-z][a-z0-9+.-]*:|\/\/)/i.test(href)) continue;

    let target = from;
    let fragment = "";

    if (href.startsWith("#")) {
      fragment = href.slice(1);
    } else if (href.startsWith("/")) {
      if (href !== PREFIX && !href.startsWith(`${PREFIX}/`)) {
        failures.push(`${from}: "${href}" is site-absolute but misses the ${PREFIX} base`);
        continue;
      }
      const [path, ...rest] = href.slice(PREFIX.length).split("#");
      fragment = rest.join("#");
      target = fileFor(path || "/");
      if (!present.has(target)) {
        failures.push(`${from}: "${href}" points at nothing -- no ${target} was rendered`);
        continue;
      }
    } else {
      failures.push(`${from}: "${href}" is relative, which is ambiguous under a base path`);
      continue;
    }

    checked += 1;
    if (fragment && !ids.get(target)?.has(fragment)) {
      failures.push(`${from}: "${href}" -- ${target} has no element with id "${fragment}"`);
    }
  }
}

if (failures.length) {
  console.error(`\nBroken links in ${DIST}:\n`);
  for (const failure of [...new Set(failures)].sort()) console.error(`  ${failure}`);
  console.error(`\n${failures.length} broken, ${checked} good, over ${pages.length} pages.`);
  process.exit(1);
}

console.log(`prestige: ${checked} internal links and images resolve across ${pages.length} pages`);
