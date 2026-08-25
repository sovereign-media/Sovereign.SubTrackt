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
// There were three cards until the research section left the site, and then a paragraph under them
// explaining where it had gone. The paragraph read as an apology for a link. What is left is one
// footer line: the two off-site destinations, named, and the version. Anything longer here is
// something the guide says better two clicks in.
//
// Plain anchors carrying `site.base`, not router `Link`s, for the same reason the old redirect used
// one: the route ids are generated during a Vite run and typing a computed `to` against them buys
// nothing here, and this is what Prestige's own header emits for the link that lands on this page.
const SECTIONS = [
  {
    href: `${site.base}guide/${site.guide[0]}`,
    label: "How it works",
    lede: "Eight short pages, assuming nothing. What an image-based subtitle is, why this isn't OCR, how it measures up against five other tools, what the pipeline does with a track, and what it refuses to guess at.",
    cta: "Start here to understand it",
  },
  {
    href: `${site.base}usage/${site.usage[0]}`,
    label: "Usage",
    lede: "Install it, build a reference set, read a track. Then one page per command with every flag, and worked examples of whole jobs.",
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
          Glyph-based subtitle extraction, without a general OCR engine.
        </p>
        <p className="mt-4 text-default-500">
          Read image-based subtitles from MKV containers at blistering speed, and accurately
          enough to hold its own against OCR where the typeface is matched. SubTrackt reads Blu-ray
          PGS and DVD VOBSUB streams, and outputs plain text. Accuracy here is a judgement rather
          than a measurement: every figure on this site was scored against a transcript a person
          typed.
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
        v1.0 ·{" "}
        <a className="underline" href={site.repo}>
          Source
        </a>{" "}
        ·{" "}
        <a className="underline" href={`${site.repo}/tree/main/docs`}>
          Research
        </a>
      </p>
    </main>
  );
}
