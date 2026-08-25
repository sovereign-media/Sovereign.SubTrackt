// A link on every heading, so a section can be pointed at.
//
// The markdown pipeline already runs `rehype-slug`, so every heading arrives with the `id` a
// fragment needs -- the sidebar's "on this page" list has been using them all along. What was
// missing is any way for a *reader* to get one: the id is in the HTML and nowhere in the page, so
// linking someone to "the accuracy gate" meant linking them to the page and telling them to scroll.
//
// This is done in the browser rather than in the markdown for the same reason the link rewrite is
// done in `scripts/content.mjs`: Prestige accepts a `markdown.rehypePlugins` list in its config,
// documents it in its own reference, and never passes it to the compiler. Version 0.15.0, checked.
// `rehype-autolink-headings` is the tool for this job and there is no way to install it.
//
// Doing it in the generator instead is worse rather than merely different. The generator emits
// markdown, so the anchor would have to be written into the heading text -- where it would land in
// the slug that names it, and in the "on this page" list, both of which are computed from that
// text.

import { useEffect } from "react";

/** Headings that get one. Not `h1`: it names the page, and the page already has a URL. */
const HEADINGS = ".prose :is(h2, h3, h4, h5, h6)[id]:not([data-anchored])";

export function HeadingLinks() {
  useEffect(() => {
    const add = () => {
      for (const heading of document.querySelectorAll<HTMLElement>(HEADINGS)) {
        // Set before appending. The observer below wakes on the append, and an unguarded second
        // pass would append a second anchor, wake it again, and not stop.
        heading.dataset.anchored = "";

        const link = document.createElement("a");
        link.className = "heading-anchor";
        link.href = `#${heading.id}`;
        link.textContent = "#";
        // The visible text is a lone `#`, which read aloud in a list of links is useless. The
        // heading's own words are what tells one from another.
        link.setAttribute("aria-label", `Link to this section: ${heading.textContent?.trim()}`);
        heading.append(link);
      }
    };

    add();
    // Client-side navigation swaps the article without remounting this, so the headings of the
    // second page a reader visits would otherwise have no links.
    const observer = new MutationObserver(add);
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);

  return null;
}
