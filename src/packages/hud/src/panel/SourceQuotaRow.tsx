import { useState } from "react";
import type { Quota } from "@agent-hud/protocol";
import { quotaDisplayLabel, quotaPercent } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { quotaMetric } from "../quota-metrics";
import { quotaShortName } from "../model";
import { usePanelServices } from "../PanelContext";
import { useGround } from "../ground";
import { levelFill, levelText, type Level } from "../tokens";
import { FittingName } from "./FittingName";
import { palette, tabular, type Measure } from "./theme";

/** One quota window as a bar row: its name, the bar with burn's projection and
 * budget mark, its value and its reset or detail. */
export function SourceQuotaRow({
  row,
  measure,
  now,
  light,
  showReset,
  onStats,
}: {
  row: Quota;
  measure: Measure;
  now: number;
  light: boolean;
  showReset: boolean;
  onStats?: () => void;
}) {
  const ground = useGround(light);
  const colors = palette(ground);
  const services = usePanelServices();
  const [hovered, setHovered] = useState(false);
  const clickable = Boolean(services?.selectProvider || onStats);
  const activate = () => {
    services?.selectProvider?.(row.vendor);
    onStats?.();
  };
  const format = useFormat();
  const reading = quotaMetric(row, measure, now, format);
  const pct = quotaPercent(row.usedPercent, format.quotaDisplay);
  const projection =
    measure === "burn" &&
    reading.projectedPercent != null &&
    reading.projectedPercent > (row.usedPercent ?? 0)
      ? quotaPercent(reading.projectedPercent, format.quotaDisplay)
      : null;
  const solidPercent =
    format.quotaDisplay === "remaining" && projection != null
      ? projection
      : pct;
  const budget = quotaPercent(reading.budgetPercent, format.quotaDisplay);
  const quotaLevel: Level =
    row.status === "critical" || row.status === "warning" ? row.status : "ok";
  const level: Level =
    measure === "quota"
      ? quotaLevel
      : measure === "burn" && reading.runsOut
        ? "warning"
        : "ok";
  const known = reading.available && row.status !== "stale";
  const color = known ? levelFill(level, ground) : colors.tertiary;
  // The value is text: the level's text shade (darker on a light HUD).
  const valueColor = levelText(level, ground);
  const showsTrack = measure === "quota" || measure === "burn";
  return (
    <div
      className="hud-source-quota-row"
      title={`${row.name} · ${reading.hint}`}
      role={clickable ? "button" : undefined}
      tabIndex={clickable ? 0 : undefined}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={clickable ? activate : undefined}
      onKeyDown={
        clickable
          ? (e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                activate();
              }
            }
          : undefined
      }
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "var(--hud-row-padding, 4px) 6px",
        fontSize: 13,
        lineHeight: "20px",
        minHeight: "calc(20px + var(--hud-row-padding, 4px) * 2)",
        borderRadius: 6,
        cursor: clickable ? "pointer" : undefined,
        background: hovered && clickable ? colors.selected : "transparent",
        transition: "background 0.12s ease",
      }}
    >
      <span
        style={{
          display: "block",
          width: 8,
          height: 8,
          flex: "0 0 8px",
          borderRadius: "50%",
          background: color,
          boxShadow: light || !known ? undefined : `0 0 8px ${color}`,
        }}
      />
      <FittingName
        name={row.name}
        short={quotaShortName(row)}
        style={{
          flex: "0 0 152px",
          width: 152,
          fontWeight: 600,
          // Leave room for Windows font ink below the line's baseline even
          // when the fitting label clips long names horizontally.
          paddingBlock: 2,
          marginBlock: -2,
          color: colors.text,
        }}
      />
      {showsTrack ? (
        <span
          role="meter"
          aria-label={`${row.name} · ${quotaDisplayLabel(format.quotaDisplay, format.language)}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct ?? undefined}
          style={{
            position: "relative",
            height: 4,
            flex: "1 1 0",
            minWidth: 0,
            background: colors.track,
            borderRadius: 4,
            overflow: "hidden",
          }}
        >
          {projection != null && pct != null && (
            <i
              style={{
                position: "absolute",
                top: 0,
                left: `${Math.min(pct, projection)}%`,
                display: "block",
                height: 4,
                width: `${Math.abs(projection - pct)}%`,
                borderRadius: 4,
                background: reading.runsOut ? colors.warning : colors.secondary,
                opacity: 0.38,
              }}
            />
          )}
          <i
            style={{
              position: "absolute",
              inset: "0 auto 0 0",
              display: "block",
              height: 4,
              width: `${solidPercent ?? 0}%`,
              borderRadius: 4,
              background:
                row.status === "stale"
                  ? colors.secondary
                  : levelFill(quotaLevel, ground),
            }}
          />
          {measure === "burn" && budget != null && (
            // Where a steady pace through the cycle would have the bar now.
            <i
              aria-hidden
              style={{
                position: "absolute",
                top: 0,
                bottom: 0,
                left: `calc(${budget}% - 0.75px)`,
                display: "block",
                width: 1.5,
                background: colors.text,
                opacity: 0.6,
              }}
            />
          )}
        </span>
      ) : (
        <span aria-hidden="true" style={{ flex: "1 1 0", minWidth: 0 }} />
      )}
      <span
        style={{
          ...tabular,
          minWidth: 62,
          flexShrink: 0,
          textAlign: "right",
          fontSize: reading.available ? 13 : 12,
          fontWeight: 600,
          whiteSpace: "nowrap",
          color: known ? valueColor : colors.secondary,
        }}
      >
        {reading.value}
      </span>
      {(showReset || measure !== "quota") && (
        <span
          style={{
            ...tabular,
            flex: "0 0 96px",
            width: 96,
            textAlign: "right",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            fontSize: 12,
            fontWeight: 400,
            color:
              measure === "burn" && reading.runsOut
                ? colors.warningText
                : colors.secondary,
          }}
        >
          {reading.detail}
        </span>
      )}
    </div>
  );
}
