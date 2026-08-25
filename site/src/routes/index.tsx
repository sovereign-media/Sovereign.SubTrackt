import { createFileRoute } from "@tanstack/react-router";
import site from "../../site.json" with { type: "json" };

// The root used to meta-refresh into `/guide/what-this-is`, on the reasoning that a landing page
// would only be the README in larger type. That stopped being right once the header's own brand
// link pointed here: clicking `SubTrackt` bounced the reader into `How it works` through a full
// page reload, which reads as a bug rather than as navigation.
//
// So the root is a page now, and it is deliberately not a hero: three sentences and the ways in,
// sized so the whole thing is one screen. The sections are listed here rather than derived from
// `site.json`, because what each one is *for* is a sentence somebody has to write, and the manifest
// carries ordering rather than prose.
//
// There were three cards until the research section left the site. The measurements are still
// worth reaching from here, so they are the line under the cards rather than a card of their own:
// the link leaves for the repository, and a card that quietly does that is a card that lies.
//
// Plain anchors carrying `site.base`, not router `Link`s, for the same reason the old redirect used
// one: the route ids are generated during a Vite run and typing a computed `to` against them buys
// nothing here, and this is what Prestige's own header emits for the link that lands on this page.
const SECTIONS = [
  {
    href: `${site.base}guide/${site.guide[0]}`,
    label: "How it works",
    lede: "Seven short pages assuming nothing: what an image-based subtitle is, why this is not OCR, how it measures up against five other tools, and what it refuses to guess at.",
    cta: "Start here to understand it",
  },
  {
    href: `${site.base}usage/${site.usage[0]}`,
    label: "Usage",
    lede: "Install it, build a reference set, read a track. Then one page per command, with every flag.",
    cta: "Start here to run it",
  },
];

export const Route = createFileRoute("/")({
  head: () => ({
    meta: [
      {
        name: "description",
        content:
          "Extract plain text from bitmap image-based subtitle streams — Blu-ray PGS and DVD VOBSUB — without a general OCR engine.",
      },
    ],
  }),
  component: Home,
});

function Home() {
  return (
    <main className="container mx-auto px-6 py-16 lg:py-24">
      <div className="max-w-3xl">
        <h1 className="text-3xl font-medium text-default-900 lg:text-4xl">
          {site.title}
        </h1>
        <p className="mt-4 text-lg text-default-700">
          Extract plain text from bitmap image-based subtitle streams — Blu-ray
          PGS and DVD VOBSUB — without human intervention, and without a general
          OCR engine.
        </p>
        <p className="mt-4 text-default-500">
          A shape it cannot read comes back as an unread glyph rather than as a
          plausible guess, because an unmatched character is a fact a caller can
          act on and a confident wrong one is not. It ships with no reference
          glyph set embedded, on purpose — you build one from fonts you already
          have.
        </p>
      </div>

      <div className="mt-12 grid gap-4 lg:grid-cols-2">
        {SECTIONS.map((section) => (
          <a
            key={section.href}
            href={section.href}
            className="block rounded-lg border border-default-200 bg-default-50 p-6 no-underline hover:bg-default-100"
          >
            <h2 className="text-lg font-medium text-default-900">
              {section.label}
            </h2>
            <p className="mt-2 text-sm text-default-500">{section.lede}</p>
            <p className="mt-4 text-sm font-medium text-default-700">
              {section.cta} →
            </p>
          </a>
        ))}
      </div>

      <p className="mt-12 text-sm text-default-500">
        Every decision above was measured before it was made. Fourteen
        write-ups — typeface surveys, error censuses, accuracy across a real
        library, and the approaches that were tried and failed — are in{" "}
        <a className="underline" href={`${site.repo}/tree/main/docs`}>
          docs/
        </a>{" "}
        in the repository.
      </p>

      <p className="mt-4 text-sm text-default-500">
        <a className="underline" href={site.repo}>
          Source on GitHub
        </a>
        . Version 1.0: the command-line surface is frozen, and flags and output
        formats change on a major.
      </p>
    </main>
  );
}
