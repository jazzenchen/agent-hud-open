import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { card, type StatsTheme } from "../StatsTheme";
import { usePendingLook, type Pending } from "./pending";

/** An element's width, following every resize of the window. The ref is a
 * callback, so an element swapped for another (a placeholder for its
 * content) is measured too. */
export function useWidth<T extends HTMLElement = HTMLDivElement>() {
  const [node, setNode] = useState<T | null>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    if (!node) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth(Math.round(entry.contentRect.width)),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, [node]);
  return { ref: setNode, width };
}

/** The Usage page's frame: its type, its ground, and the colours its
 * cards read from CSS (the frame of sections without a Panel of their own,
 * and the wash across a card waiting for new filters). */
export function pageStyle(theme: StatsTheme, light: boolean): CSSProperties {
  return {
    display: "flex",
    flexDirection: "column",
    gap: 14,
    minWidth: 0,
    fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
    fontSize: 13,
    lineHeight: 1.35,
    color: `var(--statistics-text, ${theme.text})`,
    background: `var(--statistics-ground, ${theme.ground})`,
    ["--stats-border" as string]: theme.border,
    ["--stats-card" as string]: `var(--statistics-card, ${theme.card})`,
    ["--stats-wash-color" as string]: light
      ? "rgba(0,0,0,.035)"
      : "rgba(255,255,255,.045)",
    ["--stats-edge-color" as string]: light
      ? "rgba(0,0,0,.16)"
      : "rgba(255,255,255,.24)",
  };
}

/** One card of the overview: a title, a line of context, controls on the
 * right, and its content. `area` names its place in the page's grid;
 * `pending`, the answers it waits for when a filter changes. */
export function Panel({
  title,
  meta,
  actions,
  area,
  pending = [],
  theme,
  children,
  style,
}: {
  title: ReactNode;
  meta?: ReactNode;
  actions?: ReactNode;
  area: string;
  pending?: (Pending | null)[];
  theme: StatsTheme;
  children: ReactNode;
  style?: CSSProperties;
}) {
  const look = usePendingLook(...pending);
  return (
    <section
      className={`stats-panel stats-area-${area} ${look.className}`}
      style={{
        ...card(theme),
        padding: "14px 16px 16px",
        display: "grid",
        gap: 12,
        alignContent: "start",
        ...look.style,
        ...style,
      }}
    >
      <header
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: 12,
          flexWrap: "wrap",
          minWidth: 0,
        }}
      >
        <div style={{ display: "grid", gap: 2, minWidth: 0 }}>
          <strong style={{ fontSize: 13, fontWeight: 600 }}>{title}</strong>
          {meta && (
            <span style={{ fontSize: 11, color: theme.secondary }}>{meta}</span>
          )}
        </div>
        {actions}
      </header>
      {children}
    </section>
  );
}
