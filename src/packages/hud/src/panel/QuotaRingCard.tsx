/* One account's windows as a ring with its legend: concentric rings or a
 * segmented ring on the left, then a line per window, its name on the left
 * and its figure and reset on the right. Pointing at a line lights its arc. */
import { useState } from "react";
import { useReducedMotion } from "motion/react";
import type { Quota } from "@agent-hud/protocol";
import { useFormat } from "@agent-hud/ui/format-context";
import {
  quotaDisplayLabel,
  quotaPercent,
  quotaPercentLabel,
} from "@agent-hud/ui/format";
import { quotaMetric } from "../quota-metrics";
import { quotaShortName } from "../model";
import { usePanelServices } from "../PanelContext";
import { useGround } from "../ground";
import { MAX_USAGE_ITEMS, type RingStyle } from "../usage-meter/arcs";
import { UsageRing } from "../usage-meter/UsageRing";
import { FittingName } from "./FittingName";
import { palette, type Measure } from "./theme";
import { levelFill, markLevelText } from "../tokens";

/** The ring's diameter and line for each `ringSize`. */
const RINGS = {
  small: { size: 40, width: 4 },
  medium: { size: 52, width: 5 },
  large: { size: 60, width: 6 },
} as const;

export function QuotaRingCard({
  rows,
  style,
  ringSize = "medium",
  measure,
  now,
  light,
  showReset,
  onStats,
}: {
  rows: Quota[];
  style: RingStyle;
  ringSize?: keyof typeof RINGS;
  measure: Measure;
  now: number;
  light: boolean;
  showReset: boolean;
  onStats?: () => void;
}) {
  const ground = useGround(light),
    colors = palette(ground),
    format = useFormat(),
    services = usePanelServices(),
    reduceMotion = useReducedMotion() ?? false,
    [lit, setLit] = useState<number | null>(null);
  const ring = RINGS[ringSize] ?? RINGS.medium;
  const drawn = rows.slice(0, MAX_USAGE_ITEMS);
  // The ring is a fill; its figure and the legend's figures are text.
  const fill = (row: Quota) =>
    row.status === "stale"
      ? colors.secondary
      : levelFill(row.status === "normal" ? "ok" : row.status, ground);
  const figure = (row: Quota) =>
    row.status === "stale"
      ? colors.secondary
      : markLevelText(row.status, ground);
  const tightest = drawn.reduce<Quota | undefined>(
    (a, b) =>
      b.usedPercent != null && (!a || b.usedPercent > (a.usedPercent ?? -1))
        ? b
        : a,
    undefined,
  );
  const clickable = Boolean(services?.selectProvider || onStats);
  const open = () => {
    services?.selectProvider?.(rows[0].vendor);
    onStats?.();
  };
  return (
    <div
      className="hud-quota-ring-card"
      role={clickable ? "button" : undefined}
      tabIndex={clickable ? 0 : undefined}
      onClick={clickable ? open : undefined}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 20,
        padding: "2px 4px",
        borderRadius: 6,
        cursor: clickable ? "pointer" : undefined,
      }}
    >
      <div
        role="img"
        aria-label={drawn
          .map(
            (row) =>
              `${row.name} · ${quotaDisplayLabel(format.quotaDisplay, format.language)} · ${quotaPercentLabel(row.usedPercent, format.quotaDisplay)}`,
          )
          .join(", ")}
        style={{
          position: "relative",
          width: ring.size,
          height: ring.size,
          flex: "none",
        }}
      >
        <UsageRing
          style={style}
          items={drawn.map((row) => ({
            percent: quotaPercent(row.usedPercent, format.quotaDisplay),
            color: fill(row),
          }))}
          spec={{
            size: ring.size,
            width: ring.width,
            hole: ring.size * 0.56,
            gap: 3,
          }}
          track={colors.track}
          highlight={lit}
          reduceMotion={reduceMotion}
          styles={{ position: "absolute", inset: 0 }}
        />
        <span
          style={{
            position: "absolute",
            inset: 0,
            display: "grid",
            placeItems: "center",
            fontSize: ring.size >= 60 ? 13 : 11,
            fontWeight: 600,
            fontVariantNumeric: "tabular-nums",
            color: tightest ? figure(tightest) : colors.secondary,
          }}
        >
          {tightest
            ? quotaPercentLabel(tightest.usedPercent, format.quotaDisplay)
            : "--"}
        </span>
      </div>
      <div
        onMouseLeave={() => setLit(null)}
        style={{
          flex: 1,
          minWidth: 0,
          display: "grid",
          gridTemplateColumns: "minmax(0, 1fr) auto auto",
          columnGap: 18,
          rowGap: 3,
          alignItems: "center",
          fontSize: 12,
        }}
      >
        {rows.map((row, i) => {
          const metric = quotaMetric(row, measure, now, format),
            dim = lit != null && lit !== i,
            cell = {
              opacity: dim ? 0.45 : 1,
              transition: reduceMotion ? undefined : "opacity 0.18s",
            };
          return (
            <div
              key={row.id}
              title={metric.hint}
              onMouseEnter={() => setLit(i < MAX_USAGE_ITEMS ? i : null)}
              style={{ display: "contents" }}
            >
              <FittingName
                name={row.name}
                short={quotaShortName(row)}
                style={{ ...cell, color: colors.text }}
              />
              <strong
                style={{
                  ...cell,
                  color: figure(row),
                  fontWeight: 600,
                  fontVariantNumeric: "tabular-nums",
                  textAlign: "right",
                }}
              >
                {metric.value}
              </strong>
              <span
                style={{
                  ...cell,
                  fontSize: 11,
                  color: colors.secondary,
                  textAlign: "right",
                  whiteSpace: "nowrap",
                }}
              >
                {showReset || measure !== "quota" ? metric.detail : ""}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
