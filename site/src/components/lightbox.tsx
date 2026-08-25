// Click a diagram to see it bigger.
//
// The five diagrams on `How it works` are authored 640 wide, because that is roughly what the
// prose column gives them and a picture scaled to half its type size is a picture nobody reads.
// That width is right for reading *past*, and small for reading *into* -- a 16x16 grid or a
// distance bar rewards a closer look. This is the closer look.
//
// It is a delegated listener on `document` rather than an `img` override in the markdown
// pipeline, and that is forced rather than chosen: the route files that render a page are
// generated into `src/routes/(prestige)/` by the Vite plugin and gitignored, so there is nowhere
// to hand an MDX `components` map. Delegation reaches every image on every page, including pages
// that do not exist yet, and needs no cooperation from the generator.
//
// Everything here runs after hydration. The pages are prerendered, so the server render must be
// exactly nothing -- which it is, because `shown` starts null and the tagging happens in an
// effect.

import { useCallback, useEffect, useRef, useState } from "react";

/** What the overlay is showing. `null` is closed, and is the only state the server ever renders. */
type Shown = { src: string; alt: string };

/** An image is enlargeable if it is part of the rendered markdown. Nothing else on the page is. */
function contentImage(target: EventTarget | null): HTMLImageElement | null {
  const element = target as HTMLElement | null;
  if (!element || element.tagName !== "IMG") return null;
  return element.closest(".prose") ? (element as HTMLImageElement) : null;
}

export function Lightbox() {
  const [shown, setShown] = useState<Shown | null>(null);
  // Where to put focus back. A reader who opened this from the keyboard has to land somewhere
  // when it closes, and the sensible somewhere is the image they were on.
  const opener = useRef<HTMLElement | null>(null);

  const open = useCallback((image: HTMLImageElement) => {
    opener.current = image;
    setShown({ src: image.currentSrc || image.src, alt: image.alt });
  }, []);

  const close = useCallback(() => {
    setShown(null);
    opener.current?.focus();
    opener.current = null;
  }, []);

  // Make the images reachable without a mouse. An `<img>` is not focusable on its own, so this
  // has to be applied to the elements themselves -- and applied again after every client-side
  // navigation, since the router swaps the content without remounting this component.
  useEffect(() => {
    const tag = () => {
      for (const image of document.querySelectorAll<HTMLImageElement>(
        ".prose img:not([data-zoomable])",
      )) {
        image.dataset.zoomable = "";
        image.tabIndex = 0;
        image.setAttribute("role", "button");
      }
    };
    tag();
    const observer = new MutationObserver(tag);
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const onClick = (event: MouseEvent) => {
      // Modified clicks and anything but the primary button belong to the browser: a reader
      // middle-clicking a diagram wants it in a new tab, and taking that away is a regression.
      if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) {
        return;
      }
      const image = contentImage(event.target);
      if (!image) return;
      event.preventDefault();
      open(image);
    };

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setShown((current) => {
          if (!current) return null;
          opener.current?.focus();
          opener.current = null;
          return null;
        });
        return;
      }
      if (event.key !== "Enter" && event.key !== " ") return;
      const image = contentImage(document.activeElement);
      if (!image) return;
      // Space scrolls the page by default, which is the wrong answer once the image has focus.
      event.preventDefault();
      open(image);
    };

    document.addEventListener("click", onClick);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("click", onClick);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  // Hold the page still underneath. Without this the overlay stays put and the article scrolls
  // behind it, which reads as the picture having come loose from the page.
  useEffect(() => {
    if (!shown) return;
    const previous = document.documentElement.style.overflow;
    document.documentElement.style.overflow = "hidden";
    return () => {
      document.documentElement.style.overflow = previous;
    };
  }, [shown]);

  if (!shown) return null;

  return (
    // Three nested elements, and each one is load-bearing.
    //
    // The outer element is what scrolls, because a diagram enlarged enough to be worth enlarging
    // does not fit the window. Fitting it to the viewport instead is the obvious thing and it is
    // the wrong thing here: the tallest of these is 640x824, so on a laptop "fit" enlarges it by
    // about six percent, which is not a reason to have clicked.
    //
    // The middle element is the backdrop you click to dismiss, and it is a separate element so
    // that a click on the *scrollbar* -- which belongs to the outer one -- does not dismiss.
    <div
      data-lightbox=""
      role="dialog"
      aria-modal="true"
      aria-label={shown.alt || "Enlarged diagram"}
      className="fixed inset-0 z-50 overflow-y-auto bg-default-950/85 backdrop-blur-sm"
    >
      <div
        onClick={close}
        className="flex min-h-full cursor-zoom-out items-center justify-center p-4 sm:p-8"
      >
        <img
          src={shown.src}
          alt={shown.alt}
          // Width is set rather than capped, which is the whole point. An `<img>` holding a
          // 640-wide SVG has that as its intrinsic size, so `max-width` can only ever make it
          // smaller -- `max-w-full` here would enlarge nothing at all.
          className="w-[min(96vw,1100px)] cursor-default"
          onClick={(event) => event.stopPropagation()}
        />
      </div>
      <button
        type="button"
        onClick={close}
        autoFocus
        // Fixed rather than absolute, so it is still reachable at the bottom of a long diagram.
        className="fixed top-4 right-4 rounded-md border border-default-700 bg-default-900/80 px-3 py-1.5 text-sm text-default-100 hover:bg-default-800"
      >
        Close
      </button>
    </div>
  );
}
