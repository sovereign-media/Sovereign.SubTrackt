import type { PrestigeShellProps } from "@lonik/prestige/ui";
import { PrestigeShell } from "@lonik/prestige/ui";
import {
  createRootRoute,
  HeadContent,
  Outlet,
  Scripts,
} from "@tanstack/react-router";
import config from "virtual:prestige/config";
import site from "../../site.json" with { type: "json" };
import appCss from "../styles.css?url";

const options: PrestigeShellProps = {
  copyright: () => (
    <span>
      MIT.{" "}
      <a className="underline" href={site.repo}>
        Source on GitHub
      </a>
      . Built with{" "}
      <a className="underline" href="https://github.com/lukonik/prestige">
        Prestige
      </a>
      .
    </span>
  ),
};

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: config.title },
    ],
    links: [
      { rel: "stylesheet", href: appCss },
      // Written out rather than left as `/favicon.svg`, which would resolve against the domain
      // root and 404 on a project Pages site.
      { rel: "icon", type: "image/svg+xml", href: `${site.base}favicon.svg` },
    ],
  }),
  component: () => (
    <html lang="en" suppressHydrationWarning>
      <head>
        <HeadContent />
      </head>
      <body>
        <PrestigeShell options={options}>
          <Outlet />
        </PrestigeShell>
        <Scripts />
      </body>
    </html>
  ),
});
