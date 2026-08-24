import { createFileRoute } from "@tanstack/react-router";
import site from "../../site.json" with { type: "json" };

const ENTRY = `${site.base}guide/${site.guide[0]}`;

// There is no landing page on purpose: a hero restating the README in larger type is not one of the
// two audiences this site has. The root sends a reader straight into `How it works`.
//
// A meta refresh rather than a router redirect, because this is a static host. A redirect thrown in
// `beforeLoad` needs either a server to answer with a 302 or JavaScript to have loaded; the refresh
// works with neither, and the anchor below works even if a browser has refreshes turned off.
export const Route = createFileRoute("/")({
  head: () => ({
    meta: [{ "http-equiv": "refresh", content: `0; url=${ENTRY}` }],
  }),
  component: () => (
    <main className="p-8">
      <a className="underline" href={ENTRY}>
        Continue to {site.title}
      </a>
    </main>
  ),
});
