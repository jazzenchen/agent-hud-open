import type { ReactNode } from "react";
import { clipped, compact, numeric, plain, type Theme } from "./SessionTheme";

/** A picked row's ground, the list's selected row's, reaching a little past
 * the row so its text keeps its place. */
export const picked = (theme: Theme) => ({
  background: theme.light ? "rgba(0,0,0,.06)" : "rgba(255,255,255,.09)",
  boxShadow: `0 0 0 4px ${theme.light ? "rgba(0,0,0,.06)" : "rgba(255,255,255,.09)"}`,
  borderRadius: 4,
});

/** One row of a ranked list: a name, its value and share, and a bar for its share of the whole. */
export interface ShareRow {
  id: string;
  label: ReactNode;
  value: number;
  /** The text after the value: its share, or anything else. */
  note?: string;
  color?: string;
  title?: string;
}

/**
 * Labelled bars, one per row, each as long as its share of `whole`: the name
 * and the figures sit on the bar's line, so nothing needs a hover. The row
 * open in the next column is marked as the list marks its session.
 */
export function ShareRows({
  rows,
  whole,
  theme,
  onSelect,
  selected = null,
}: {
  rows: ShareRow[];
  whole: number;
  theme: Theme;
  onSelect?: (id: string) => void;
  selected?: string | null;
}) {
  return (
    <div style={{ display: "grid", gap: 7, minWidth: 0 }}>
      {rows.map((row) => {
        const content = (
          <>
            <span
              style={{
                display: "flex",
                alignItems: "baseline",
                gap: 8,
                minWidth: 0,
                fontSize: 12,
              }}
            >
              <span style={{ ...clipped, flex: 1 }}>{row.label}</span>
              <strong style={{ fontWeight: 600, ...numeric }}>
                {compact(row.value)}
              </strong>
              {row.note && (
                <span
                  style={{
                    color: theme.secondary,
                    fontSize: 11,
                    minWidth: 30,
                    textAlign: "right",
                    ...numeric,
                  }}
                >
                  {row.note}
                </span>
              )}
            </span>
            <span
              aria-hidden="true"
              style={{
                display: "block",
                height: 4,
                borderRadius: 2,
                background: theme.track,
                overflow: "hidden",
                marginTop: 3,
              }}
            >
              <span
                style={{
                  display: "block",
                  height: "100%",
                  width: `${whole > 0 ? Math.min(100, (row.value / whole) * 100) : 0}%`,
                  minWidth: row.value > 0 ? 2 : 0,
                  background: row.color ?? theme.secondary,
                  borderRadius: 2,
                  transition: "width 240ms cubic-bezier(0.22, 1, 0.36, 1)",
                }}
              />
            </span>
          </>
        );
        return onSelect ? (
          <button
            key={row.id}
            type="button"
            title={row.title}
            className="session-share-row"
            aria-current={row.id === selected || undefined}
            onClick={() => onSelect(row.id)}
            style={{
              ...plain,
              display: "block",
              width: "100%",
              minWidth: 0,
              ...(row.id === selected ? picked(theme) : {}),
            }}
          >
            {content}
          </button>
        ) : (
          <div key={row.id} title={row.title} style={{ minWidth: 0 }}>
            {content}
          </div>
        );
      })}
    </div>
  );
}
