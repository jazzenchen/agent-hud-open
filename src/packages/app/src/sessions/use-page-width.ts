import { useLayoutEffect, useRef, useState } from "react";

/**
 * The page's whole width, read before the page is painted, again whenever it
 * is shown, and on every resize. A hidden page measures nothing, so it keeps
 * its last width and its columns stay as they were.
 */
export function usePageWidth<T extends HTMLElement>(shown: boolean) {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  const keep = (value: number) => {
    if (value > 0) setWidth(Math.round(value));
  };
  useLayoutEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new ResizeObserver(([entry]) =>
      keep(entry.borderBoxSize?.[0]?.inlineSize ?? entry.contentRect.width),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  useLayoutEffect(() => {
    if (shown && ref.current) keep(ref.current.getBoundingClientRect().width);
  }, [shown]);
  return { ref, width };
}
