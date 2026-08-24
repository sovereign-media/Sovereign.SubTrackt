#!/usr/bin/env node
import { generate, OUT_DIR } from "./content.mjs";

const pages = await generate();
console.log(`prestige: generated ${pages.length} pages in ${OUT_DIR}`);
