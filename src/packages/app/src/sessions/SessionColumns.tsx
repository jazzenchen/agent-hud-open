import type { CSSProperties, ReactNode, Ref } from "react";
import type { SessionColumn } from "./session-layout";
import type { Theme } from "./SessionTheme";

/**
 * The Sessions page's columns side by side, each scrolling on its own under
 * the toolbar; or, in one column, the one shown, scrolling with the window.
 */
export function SessionColumns({
  sectionRef,
  label,
  style,
  split,
  shown,
  tracks,
  pickKey,
  theme,
  columns,
}: {
  sectionRef: Ref<HTMLElement>;
  label: string;
  style: CSSProperties;
  split: boolean;
  shown: SessionColumn[];
  tracks: string;
  /** Which pick is open: a new one starts at its column's top. */
  pickKey: string;
  theme: Theme;
  columns: Record<SessionColumn, ReactNode>;
}) {
  const divider = `1px solid ${theme.divider}`;
  const scrolls: CSSProperties = {
    overflowY: "auto",
    overflowX: "hidden",
    minHeight: 0,
    minWidth: 0,
  };
  const column = (name: SessionColumn, index: number): CSSProperties =>
    name === "list"
      ? {
          ...scrolls,
          paddingRight: 16,
          paddingBottom: 24,
          borderRight: divider,
        }
      : {
          ...scrolls,
          // The page's own side stands left of the first column.
          padding: index === 0 ? "4px 22px 24px 0" : "4px 22px 24px",
          borderLeft: name === "pick" ? divider : undefined,
        };
  return (
    <section
      ref={sectionRef}
      className="session-statistics-native"
      style={
        split
          ? {
              ...style,
              padding: "0 0 0 22px",
              display: "grid",
              gridTemplateColumns: tracks,
              height: "calc(100vh - 44px)",
              overflow: "hidden",
            }
          : style
      }
      aria-label={label}
    >
      {split
        ? shown.map((name, index) => (
            <div
              key={name === "pick" ? `pick:${pickKey}` : name}
              className={`session-statistics-${name === "session" ? "detail" : name}-column`}
              style={column(name, index)}
            >
              {columns[name]}
            </div>
          ))
        : columns[shown[0]]}
    </section>
  );
}
