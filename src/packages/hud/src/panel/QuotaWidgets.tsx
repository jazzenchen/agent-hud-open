import type { PanelBlockLayout, Quota } from "@agent-hud/protocol";
import { useFormat } from "@agent-hud/ui/format-context";
import { quotaMetric } from "../quota-metrics";
import { quotaShortName } from "../model";
import { usePanelServices } from "../PanelContext";
import { useGround } from "../ground";
import { markLevelText } from "../tokens";
import { FittingName } from "./FittingName";
import { QuotaRingCard } from "./QuotaRingCard";
import { SourceQuotaRow } from "./SourceQuotaRow";
import { palette, type Measure } from "./theme";

/** QuotaCell.swift: centered equal-width ring/number lines, fixed 40/46/60 pt rings. */
export function QuotaWidgets({
  rows,
  layout,
  measure,
  now,
  light,
  showReset,
  onStats,
}: {
  rows: Quota[];
  layout: PanelBlockLayout;
  measure: Measure;
  now: number;
  light: boolean;
  showReset: boolean;
  onStats?: () => void;
}) {
  const ground = useGround(light),
    colors = palette(ground),
    services = usePanelServices(),
    format = useFormat(),
    clickable = Boolean(services?.selectProvider || onStats),
    // A layout saved before rings and segments says "ring": concentric rings.
    widget = layout.widget === "ring" ? "rings" : (layout.widget ?? "bar");
  if (widget === "rings" || widget === "segments")
    return (
      <QuotaRingCard
        style={widget}
        ringSize={layout.ringSize}
        {...{ rows, measure, now, light, showReset, onStats }}
      />
    );
  if (widget === "bar")
    return (
      <>
        {rows.map((row) => (
          <SourceQuotaRow
            key={row.id}
            {...{ row, measure, now, light, showReset, onStats }}
          />
        ))}
      </>
    );
  const columns = layout.columns ?? 4;
  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        justifyContent: "center",
        gap: 8,
        padding: "4px 6px",
      }}
    >
      {rows.map((row) => {
        const metric = quotaMetric(row, measure, now, format);
        const figure =
          row.status === "stale"
            ? colors.secondary
            : markLevelText(row.status, ground);
        return (
          <div
            key={row.id}
            title={metric.hint}
            role={clickable ? "button" : undefined}
            tabIndex={clickable ? 0 : undefined}
            onClick={
              clickable
                ? () => {
                    services?.selectProvider?.(row.vendor);
                    onStats?.();
                  }
                : undefined
            }
            style={{
              flex: `0 0 calc((100% - ${(Math.min(columns, rows.length) - 1) * 8}px) / ${Math.min(columns, rows.length)})`,
              minWidth: 0,
              display: "flex",
              flexDirection: "column",
              alignItems: "flex-start",
              gap: 5,
              fontSize: 12,
              cursor: clickable ? "pointer" : undefined,
              borderRadius: 6,
            }}
          >
            <FittingName
              name={row.name}
              short={quotaShortName(row)}
              wrap
              style={{
                alignSelf: "stretch",
                fontSize: 11,
                color: colors.secondary,
                textAlign: "center",
              }}
            />
            <strong style={{ color: figure, fontSize: 15 }}>
              {metric.value}
            </strong>
            <span
              style={{
                fontSize: 11,
                color: colors.secondary,
                textAlign: "center",
                minHeight: 13,
              }}
            >
              {showReset || measure !== "quota" ? metric.detail : ""}
            </span>
          </div>
        );
      })}
    </div>
  );
}
