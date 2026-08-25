#!/usr/bin/env node
// Asserts, over the built stylesheet, that inline code is not wearing its own backticks.
//
// `@tailwindcss/typography` styles a `<code>` span with `::before`/`::after { content: "`" }`. It
// is a reasonable default and it is wrong here, because `src/styles.css` gives code a background
// instead -- so the page was rendering `--include-outline` with the punctuation that made it code
// in the source, which is not what anyone writing the backticks expected to see.
//
// This is checked rather than remembered for the same reason `scripts/check.sh` checks the
// dependency tree: the property is breakable by a *dependency upgrade*, from a change that never
// mentions this file, and it fails silently -- a page that renders, builds green, deploys, and
// reads slightly wrong on every line that names a flag. Nobody would think to look.
//
// The override wins on specificity rather than on source order (`.prose code` beats typography's
// deliberately-zero-scoring `:where(code)`), so its presence in the output is the whole property.
// If this ever fails after an upgrade, check what the new default is before deleting the rule.

import { readdir, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SITE_DIR = dirname(dirname(fileURLToPath(import.meta.url)));
const ASSETS = join(SITE_DIR, "dist", "client", "assets");

const OVERRIDE = /\.prose\s+code:+before\s*,\s*\.prose\s+code:+after\s*\{\s*content\s*:\s*none\s*\}/;

const sheets = (await readdir(ASSETS)).filter((name) => name.endsWith(".css"));
if (sheets.length === 0) {
  console.error("prestige: no stylesheet in dist/client/assets -- did the build run?");
  process.exit(1);
}

let found = false;
for (const name of sheets) {
  if (OVERRIDE.test(await readFile(join(ASSETS, name), "utf8"))) found = true;
}

if (!found) {
  console.error(
    "\nInline code has lost its backtick override.\n\n" +
      "  Nothing in dist/client/assets sets `content: none` on .prose code::before/::after, so\n" +
      "  @tailwindcss/typography's default is painting literal backticks around every inline code\n" +
      "  span on the site. The rule lives in src/styles.css; see the note there.\n",
  );
  process.exit(1);
}

console.log(`prestige: inline code carries no backticks (${sheets.length} stylesheet(s) checked)`);
