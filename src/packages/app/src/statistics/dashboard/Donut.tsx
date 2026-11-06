import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { number, plain, row, truncate, type StatsTheme } from "../StatsTheme";
import { arcPath } from "./curves";
import { useTweens } from "../motion";
import type { ReactNode } from "react";

const SIZE = 132;
const GAP = 0.025;

/** The share of each part as a donut, its slices parted by a thin gap; the
 * hovered one lifts, the rest fade. The middle names what it adds up to. */
export function Donut({
  parts,
  total,
  hovered,
  setHovered,
  center,
  caption,
  theme,
}: {
  parts: { id: string; value: number; color: string }[];
  total: number;
  hovered: string | null;
  setHovered: (id: string | null) => void;
  center: ReactNode;
  caption: string;
  theme: StatsTheme;
}) {
  const r = SIZE / 2;
  // Slices sweep to their new shares, from nothing the first time.
  const values = useTweens(parts.map((part) => part.value));
  const sum = Math.max(
    1,
    total,
    values.reduce((all, value) => all + value, 0),
  );
  let angle = 0;
  return (
    <svg
      width={SIZE}
      height={SIZE}
      style={{ display: "block", flex: "none", overflow: "visible" }}
      onMouseLeave={() => setHovered(null)}
      aria-hidden
    >
      {parts.map((part, index) => {
        const sweep = ((values[index] ?? 0) / sum) * Math.PI * 2;
        const start = angle;
        angle += sweep;
        if (sweep <= 0) return null;
        const gap = parts.length > 1 ? Math.min(GAP, sweep / 3) : 0;
        const lifted = hovered === part.id;
        return (
          <path
            key={part.id}
            d={arcPath(
              r,
              r,
              lifted ? r + 3 : r,
              r - 18,
              start + gap / 2,
              start + sweep - gap / 2,
            )}
            fill={part.color}
            opacity={hovered && !lifted ? 0.35 : 1}
            style={{ transition: "opacity 160ms ease" }}
            onMouseEnter={() => setHovered(part.id)}
          />
        );
      })}
      <text
        x={r}
        y={r - 1}
        textAnchor="middle"
        fontSize={17}
        fontWeight={600}
        fill={theme.text}
      >
        {center}
      </text>
      <text
        x={r}
        y={r + 15}
        textAnchor="middle"
        fontSize={10}
        fill={theme.secondary}
      >
        {caption}
      </text>
    </svg>
  );
}

/** One line of the legend: a swatch and name, the share, tokens, and a line
 * with its worth and sessions. */
export function LegendRow({
  id,
  name,
  color,
  share,
  value,
  detail,
  logo,
  hovered,
  setHovered,
  onClick,
  theme,
  light,
}: {
  id: string;
  name: string;
  color: string;
  share: number;
  value: ReactNode;
  detail: string;
  logo: boolean;
  hovered: string | null;
  setHovered: (id: string | null) => void;
  onClick?: () => void;
  theme: StatsTheme;
  light: boolean;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(id)}
      onMouseLeave={() => setHovered(null)}
      style={{
        ...plain,
        display: "grid",
        gridTemplateColumns: "10px minmax(0,1fr) auto",
        columnGap: 8,
        rowGap: 1,
        alignItems: "center",
        padding: "5px 6px",
        borderRadius: 6,
        background:
          hovered === id
            ? theme.light
              ? "rgba(0,0,0,.04)"
              : "rgba(255,255,255,.06)"
            : "transparent",
        cursor: onClick ? "pointer" : "default",
      }}
    >
      <span
        aria-hidden
        style={{ width: 10, height: 10, borderRadius: 5, background: color }}
      />
      <span style={{ ...row, gap: 6, minWidth: 0 }}>
        {logo && <HUDLogo source={id} size={13} light={light} />}
        <span style={{ ...truncate, fontSize: 12, fontWeight: 500 }}>
          {name}
        </span>
      </span>
      <span style={{ ...number, fontSize: 12, fontWeight: 600 }}>
        {value}
        <span style={{ fontWeight: 400, color: theme.secondary }}>
          {" "}
          {Math.round(share * 100)}%
        </span>
      </span>
      <span />
      <span
        style={{
          ...truncate,
          ...number,
          gridColumn: "2 / 4",
          fontSize: 11,
          color: theme.secondary,
        }}
      >
        {detail}
      </span>
    </button>
  );
}
