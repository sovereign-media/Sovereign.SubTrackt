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
  // DocSearch box only when this object is present. Sixteen pages behind a sidebar do not need an
  // Algolia index that goes stale against a document that changed.
  collections: [
    {
      id: "guide",
      label: "How it works",
      defaultLink: `/guide/${site.guide[0]}`,
      // Order only. A guide page is written by hand, so its title, description and sidebar label
      // are frontmatter on the page itself; the research pages are generated from documents that
      // carry no frontmatter, which is why those carry theirs in this file instead.
      items: site.guide.map((slug) => `guide/${slug}`),
    },
    {
      id: "research",
      label: "Research",
      // The first document of the first group, which site.json puts at `architecture.md`: it is
      // the entry point of the corpus and the one a reader arriving at `Research` should land on.
      defaultLink: `/research/${site.research[0].documents[0].file.replace(/\.md$/, "")}`,
      items: site.research.map((group) => ({
        label: group.label,
        items: group.documents.map((doc) => ({
          slug: `research/${doc.file.replace(/\.md$/, "")}`,
          label: doc.label,
        })),
      })),
    },
  ],
});
