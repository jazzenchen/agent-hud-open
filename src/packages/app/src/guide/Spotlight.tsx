// Where the element the guide points at (`data-guide={anchor}`) sits in the
// window, scrolled into view and followed as the page moves; and a ring
// around it that never takes a click.
import { useEffect, useState } from "react";

export interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

export function useAnchorBox(anchor: string | null): Box | null {
  const [box, setBox] = useState<Box | null>(null);
  useEffect(() => {
    setBox(null);
    if (!anchor) return;
    const names = anchor.split(" ");
    let element: Element | null = null;
    const observer = new ResizeObserver(() => measure());
    function measure() {
      if (!element?.isConnected) return setBox(null);
      const { left, top, width, height } = element.getBoundingClientRect();
      setBox({ left, top, width, height });
    }
    // A page draws its rows once its data arrives, and draws them again when
    // it changes, so the element is looked for until the ring goes.
    function find() {
      const found =
        names
          .map((name) => document.querySelector(`[data-guide="${name}"]`))
          .find(Boolean) ?? null;
      if (found !== element) {
        if (element) observer.unobserve(element);
        element = found;
        if (found) {
          observer.observe(found);
          // A target taller than the window shows from its top.
          const tall =
            found.getBoundingClientRect().height > window.innerHeight * 0.6;
          found.scrollIntoView({
            block: tall ? "start" : "center",
            behavior: "smooth",
          });
        }
      }
      measure();
    }
    find();
    const timer = window.setInterval(find, 500);
    window.addEventListener("resize", measure);
    document.addEventListener("scroll", measure, true);
    return () => {
      window.clearInterval(timer);
      observer.disconnect();
      window.removeEventListener("resize", measure);
      document.removeEventListener("scroll", measure, true);
    };
  }, [anchor]);
  return box;
}

/** The ring; with `dim`, the rest of the window darkens around it. */
export function Ring({ box, dim = false }: { box: Box; dim?: boolean }) {
  return (
    <div
      className={`guide-spotlight${dim ? " is-dim" : ""}`}
      aria-hidden
      style={{
        left: box.left - 6,
        top: box.top - 4,
        width: box.width + 12,
        height: box.height + 8,
      }}
    />
  );
}
