import { prestige } from "@lonik/prestige/vite";
import { defineConfig } from "vite";
import tsconfigPaths from "vite-tsconfig-paths";

import { tanstackStart } from "@tanstack/react-start/plugin/vite";

import tailwindcss from "@tailwindcss/vite";
import viteReact from "@vitejs/plugin-react";

import site from "./site.json" with { type: "json" };

const config = defineConfig({
  // Project pages, not a user site: everything is served from
  // https://sovereign-media.github.io/Sovereign.SubTrackt/ rather than from the domain root.
  base: site.base,
  plugins: [
    prestige(),
    tsconfigPaths({ projects: ["./tsconfig.json"] }),
    tailwindcss(),
    tanstackStart({
      // What makes the output static HTML rather than a server. `crawlLinks` walks the anchors in
      // each rendered page, so every page reachable from the root -- which is every page in both
      // sidebars -- gets written out without being listed here. Prerendered output lands in
      // `dist/client`, which is the directory the Pages artifact action wants.
      prerender: { enabled: true, crawlLinks: true },
      // The site has no canonical host beyond the Pages URL and nothing consumes a sitemap; the
      // template's example.com default would have published a file full of wrong URLs.
      sitemap: { enabled: false },
    }),
    viteReact(),
  ],
});

export default config;
