import { defineConfig } from "@lonik/prestige/vite";
import site from "./site.json" with { type: "json" };

// The sidebar is built from `site.json` rather than written out here, because the same file is
// what `scripts/content.mjs` reads to generate `src/content/`. Two lists would drift the first
// time a page was added to one of them, and the generator fails the build if the two disagree.
export default defineConfig({
  title: site.title,
  github: site.repo,
  license: { label: "MIT", url: `${site.repo}/blob/main/LICENSE` },
  // No `algolia` key, and that omission is the whole of disabling search: Prestige renders the
  // DocSearch box only when this object is present. Fourteen pages behind two sidebars do not need
  // an Algolia index that goes stale against a page that changed.
  collections: [
    {
      id: "guide",
      label: "How it works",
      defaultLink: `/guide/${site.guide[0]}`,
      // Order only. Every page is written by hand, so its title, description and sidebar label are
      // frontmatter on the page itself and there is nothing to say about it here.
      items: site.guide.map((slug) => `guide/${slug}`),
    },
    {
      id: "usage",
      label: "Usage",
      defaultLink: `/usage/${site.usage[0]}`,
      // Same as the guide: hand-written, so each page carries its own frontmatter and this is the
      // ordering only. The order is the order you would run the commands in, with the quick start
      // in front of them, rather than alphabetical -- a reader arriving here wants the sequence.
      items: site.usage.map((slug) => `usage/${slug}`),
    },
    // There is no third collection. `docs/` used to be one, and the fourteen documents in it are
    // read in the repository now: the guide says what they found, in words that assume nothing,
    // and a reader who wants the workings follows a link out rather than a third sidebar.
  ],
});
