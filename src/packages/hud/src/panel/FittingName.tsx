import { useLayoutEffect, useRef, useState, type CSSProperties } from "react";

/** A window's name that is never cut: the full name where it fits, else Core's
 * short name (`ViewThatFits` in the Swift panel). With `wrap`, a short name
 * too long for one line wraps onto the lines it needs, as in a ring cell. */
export function FittingName({
  name,
  short,
  wrap = false,
  style,
}: {
  name: string;
  short: string;
  wrap?: boolean;
  style?: CSSProperties;
}) {
  const box = useRef<HTMLSpanElement>(null);
  const probe = useRef<HTMLSpanElement>(null);
  const [fits, setFits] = useState(true);
  useLayoutEffect(() => {
    const container = box.current;
    const full = probe.current;
    if (!container || !full) return;
    // The full name is measured on one line, whatever is shown now.
    const measure = () => setFits(full.offsetWidth <= container.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    return () => observer.disconnect();
  }, [name]);
  const shortened = !fits && short !== name;
  return (
    <span
      title={shortened ? name : undefined}
      style={{
        position: "relative",
        display: "block",
        minWidth: 0,
        overflow: "hidden",
        whiteSpace: shortened && wrap ? "normal" : "nowrap",
        ...style,
      }}
    >
      <span ref={box} style={{ display: "block", minWidth: 0 }}>
        {shortened ? short : name}
      </span>
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
      >
        {name}
      </span>
    </span>
  );
}
