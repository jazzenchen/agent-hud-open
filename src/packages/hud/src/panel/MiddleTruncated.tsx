import { useLayoutEffect, useRef, useState, type CSSProperties } from "react";

/** One line of text that, where it does not fit, loses characters from its
 * middle, so an account's address keeps its start and its domain
 * (`truncationMode(.middle)` in the Swift panel). */
export function MiddleTruncated({
  text,
  style,
}: {
  text: string;
  style?: CSSProperties;
}) {
  const box = useRef<HTMLSpanElement>(null);
  const probe = useRef<HTMLSpanElement>(null);
  const [shown, setShown] = useState(text);
  useLayoutEffect(() => {
    const container = box.current;
    const measure = probe.current;
    if (!container || !measure) return;
    const fit = () => {
      const width = container.clientWidth;
      const fits = (candidate: string) => {
        measure.textContent = candidate;
        return measure.offsetWidth <= width;
      };
      if (fits(text)) return setShown(text);
      // The most characters kept, split evenly round the ellipsis.
      let low = 0,
        high = text.length;
      while (low < high) {
        const keep = Math.ceil((low + high) / 2);
        if (fits(cut(text, keep))) low = keep;
        else high = keep - 1;
      }
      setShown(cut(text, low));
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(container);
    return () => observer.disconnect();
  }, [text]);
  return (
    <span
      ref={box}
      title={shown === text ? undefined : text}
      style={{
        position: "relative",
        display: "block",
        minWidth: 0,
        overflow: "hidden",
        whiteSpace: "nowrap",
        ...style,
      }}
    >
      {shown}
      <span
        ref={probe}
        aria-hidden
        style={{
          position: "absolute",
          left: 0,
          top: 0,
          visibility: "hidden",
          whiteSpace: "nowrap",
        }}
      />
    </span>
  );
}

function cut(text: string, keep: number) {
  if (keep <= 0) return "…";
  const head = Math.ceil(keep / 2);
  return `${text.slice(0, head)}…${text.slice(text.length - (keep - head))}`;
}
