import type { ReactNode } from "react";
import { number, plain, type StatsTheme } from "../StatsTheme";
import { Decrypt } from "../decrypt";
import { TrendLine } from "./TrendLine";
import { Bar } from "./Placeholder";

/** One tile: a label, the figure, one line of context and the figure's trend
 * across the range. Every tile has the same four rows. */
export function Figure({
  label,
  loading = false,
  value,
  suffix,
  line,
  hint,
  series,
  active,
  onClick,
  theme,
}: {
  label: string;
  /** Nothing to show yet: grey bars stand in. */
  loading?: boolean;
  value: string;
  suffix?: string | null;
  line: ReactNode;
  hint?: string;
  series: (number | null)[];
  active?: boolean;
  onClick?: () => void;
  theme: StatsTheme;
}) {
  // One figure element throughout, so the digits decrypt into place when
  // the value arrives instead of the tile swapping a bar for text.
  const figure = (
    <span
      style={{
        ...number,
        fontSize: 24,
        lineHeight: "30px",
        fontWeight: 600,
        letterSpacing: -0.2,
        overflow: "hidden",
        textOverflow: "ellipsis",
        whiteSpace: "nowrap",
      }}
    >
      <Decrypt text={loading ? null : value} waiting={theme.tertiary} />
      {!loading && suffix && (
        <span style={{ fontSize: 12, fontWeight: 400, color: theme.secondary }}>
          {" "}
          {suffix}
        </span>
      )}
    </span>
  );
  const body = loading ? (
    <>
      <span style={{ fontSize: 12, color: theme.secondary }}>{label}</span>
      {figure}
      <Bar width={128} height={10} theme={theme} style={{ margin: "2px 0" }} />
      <Bar height={22} theme={theme} style={{ marginTop: 3 }} />
    </>
  ) : (
    <>
      <span style={{ fontSize: 12, color: theme.secondary }}>{label}</span>
      {figure}
      <span
        style={{
          fontSize: 11,
          lineHeight: "15px",
          minHeight: 15,
          color: theme.secondary,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {line}
      </span>
      <TrendLine values={series} theme={theme} />
    </>
  );
  const style = {
    display: "grid",
    gridTemplateRows: "auto auto auto 28px",
    gap: 3,
    padding: "12px 16px 10px",
    borderRadius: 10,
    border: `1px solid ${theme.border}`,
    background: `var(--statistics-card, ${theme.card})`,
    minWidth: 0,
    boxShadow: active
      ? `inset 0 0 0 1.5px ${theme.light ? "rgba(0,0,0,.5)" : "rgba(255,255,255,.55)"}`
      : "none",
    transition: "box-shadow 160ms ease",
  } as const;
  const reveal = loading ? undefined : "stats-reveal";
  return onClick ? (
    <button
      type="button"
      className={reveal}
      aria-pressed={active}
      onClick={onClick}
      title={hint}
      style={{ ...plain, ...style }}
    >
      {body}
    </button>
  ) : (
    <div className={reveal} style={style} title={hint}>
      {body}
    </div>
  );
}
