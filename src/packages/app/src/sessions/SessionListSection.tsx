import type { ReactNode } from "react";
import { clipped, numeric, plain, row, type Theme } from "./SessionTheme";

/** The columns a session row and its day's header share: who and how, its turns, its figures. */
export const rowColumns = "minmax(0, 1.3fr) minmax(150px, 1fr) 132px";
export const rowGap = 18;

/** A day's header over its rows, on the rows' columns: its name and counts, then what the columns hold or, folded, what its sessions spent. */
export function DayHeader({
  title,
  count,
  open,
  folded,
  theme,
  onToggle,
  chevron,
}: {
  title: string;
  count: number;
  open: boolean;
  /** What the day spent, shown while it is folded. */
  folded: string;
  theme: Theme;
  /** Folds or opens the day; a list without days (Active) has nothing to fold. */
  onToggle?: () => void;
  chevron: ReactNode;
}) {
  const zh = Boolean(theme.zh);
  const Element = onToggle ? "button" : "div";
  return (
    <Element
      {...(onToggle
        ? { type: "button" as const, onClick: onToggle, "aria-expanded": open }
        : {})}
      style={{
        ...plain,
        cursor: onToggle ? "pointer" : "default",
        width: "100%",
        display: "grid",
        gridTemplateColumns: rowColumns,
        columnGap: rowGap,
        alignItems: "baseline",
        padding: open ? "0 15px" : "9px 15px",
        borderRadius: 10,
        border: open ? "1px solid transparent" : `1px solid ${theme.border}`,
        background: open ? "transparent" : theme.card,
        marginBottom: open ? 6 : 8,
        color: theme.secondary,
        transition:
          "background-color 180ms ease, border-color 180ms ease, padding 180ms ease",
      }}
    >
      <span style={{ ...row, gap: 6, alignItems: "baseline" }}>
        {chevron}
        <strong style={{ fontSize: 13, fontWeight: 600, color: theme.text }}>
          {title}
        </strong>
        <span style={{ fontSize: 11, ...numeric }}>{count}</span>
      </span>
      <span
        style={{
          fontSize: 11,
          opacity: open ? 1 : 0,
          transition: "opacity 180ms ease",
          ...clipped,
        }}
      >
        {zh ? "每轮新增 Token" : "New tokens per turn"}
      </span>
      <span
        style={{ fontSize: 11, textAlign: "right", ...numeric, ...clipped }}
      >
        {open
          ? zh
            ? "Token · 费用 · 上下文"
            : "Tokens · cost · context"
          : folded}
      </span>
    </Element>
  );
}
