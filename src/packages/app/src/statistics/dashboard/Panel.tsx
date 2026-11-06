import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";
import { card, type StatsTheme } from "../StatsTheme";

/** An element's width, following every resize of the window. */
export function useWidth<T extends HTMLElement = HTMLDivElement>() {
  const ref = useRef<T>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth(Math.round(entry.contentRect.width)),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

/** One card of the overview: a title, a line of context, controls on the
 * right, and its content. `area` names its place in the page's grid. */
export function Panel({
  title,
  meta,
  actions,
  area,
  theme,
  children,
  style,
}: {
  title: ReactNode;
  meta?: ReactNode;
  actions?: ReactNode;
  area: string;
  theme: StatsTheme;
  children: ReactNode;
  style?: CSSProperties;
}) {
  return (
    <section
      className={`stats-panel stats-area-${area}`}
      style={{
        ...card(theme),
        padding: "14px 16px 16px",
        display: "grid",
        gap: 12,
        alignContent: "start",
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
