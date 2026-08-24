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

console.log(`prestige: ${checked} internal links resolve across ${pages.length} pages`);
