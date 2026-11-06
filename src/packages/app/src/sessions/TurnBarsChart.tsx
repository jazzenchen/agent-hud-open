import { useState, type ReactNode } from "react";
import { Layers, RotateCw } from "lucide-react";
import {
  largestPositions,
  niceScale,
  stackTurn,
  turnTicks,
  type StackedBar,
} from "../lib/session-charts";
import { moneyShort } from "@agent-hud/ui/format";
import {
  axis,
  Halo,
  labelFont,
  Rising,
  stackedRects,
  useWidth,
} from "./SessionCharts";
import { labelReveal } from "./SessionMotion";
import { compact, numeric, type Entry, type Theme } from "./SessionTheme";

/** What the session page's turn chart measures: the new tokens of each turn, or what its calls would cost at list prices. */
export type Measure = "tokens" | "cost";

/** A value on the turn chart: tokens as "48k", money as "$1.20". */
function measureText(value: number, measure: Measure, zh: boolean) {
  return measure === "cost"
    ? moneyShort(value, "USD", zh ? "zh" : "en")
    : compact(value);
}

/** A turn's value in the chart's measure: what it added, or its list price. */
export function turnValue(entry: Entry, measure: Measure): number {
  if (measure === "cost") return entry.listCost ?? 0;
  return entry.newTokens ?? entry.input + entry.output;
}

/** A turn as a stacked bar: Core's own part and its sub-agents' (the page's parser fallback has neither). */
export function turnBar(entry: Entry): StackedBar {
  return stackTurn(
    entry.own ?? { input: entry.input, output: entry.output },
    entry.subagents,
    Boolean(entry.compacted),
  );
}

/**
 * The session page's turns, one bar each (the phone's `SessionBarsChart`):
 * new tokens stacked by kind with the sub-agents' part faded, or the list
 * price; on a round scale. The three largest turns carry their value on the
 * chart, a layers mark sits above a turn sub-agents helped and an amber
 * arrow above one whose cache had expired, a compaction is an amber dashed
 * line before its turn. No turn is marked as running: the page shows no
 * session state (owner, 2026-10-26).
 * Pointing shows `hover`; a click picks the turn.
 */
export function TurnBarsChart({
  entries,
  measure,
  selected,
  onSelect,
  hover,
  theme,
  height: total = 196,
}: {
  entries: Entry[];
  measure: Measure;
  selected: string | null;
  onSelect: (entry: Entry) => void;
  hover: (entry: Entry) => ReactNode;
  theme: Theme;
  height?: number;
}) {
  const zh = Boolean(theme.zh);
  const [ref, width] = useWidth<HTMLDivElement>();
  const [pointed, setPointed] = useState<number | null>(null);
  const labels = 18;
  const headroom = 30;
  const plot = Math.max(0, width - axis);
  const height = total - labels;
  const values = entries.map((entry) => turnValue(entry, measure));
  const scale = niceScale(Math.max(0, ...values));
  const yScale = (height - headroom) / scale.top;
  const slot = entries.length > 0 ? plot / entries.length : plot;
  const barWidth = Math.max(1, slot * 0.64);
  const picked = entries.findIndex((entry) => entry.id === selected);
  const largest = largestPositions(values, 3);
  const placed: number[] = [];
  const tops: number[] = [];
  const at = (clientX: number, element: Element) => {
    const x = clientX - element.getBoundingClientRect().left;
    if (x < 0 || x > plot || entries.length === 0) return null;
    return Math.min(entries.length - 1, Math.max(0, Math.floor(x / slot)));
  };
  const bars = entries.map((entry, index) => {
    const x = index * slot + (slot - barWidth) / 2;
    const bar =
      measure === "cost"
        ? {
            parts: [],
            total: values[index],
            compacted: Boolean(entry.compacted),
          }
        : turnBar(entry);
    let rects: ReactNode[];
    let top: number;
    if (measure === "cost") {
      const h = values[index] > 0 ? Math.max(2, values[index] * yScale) : 0;
      top = height - h;
      rects =
        h > 0
          ? [
              <Rising
                key={`c${index}`}
                x={x}
                width={barWidth}
                fill={theme.accent}
                fillOpacity={0.8}
                base={height}
                y={top + 0.4}
                height={Math.max(0.6, h - 0.8)}
              />,
            ]
          : [];
    } else {
      ({ rects, top } = stackedRects(
        bar,
        x,
        barWidth,
        height,
        yScale,
        2,
        theme,
        `t${index}`,
      ));
    }
    tops[index] = top;
    return (
      <g
        key={entry.id}
        opacity={picked < 0 || picked === index ? 1 : 0.45}
        style={{ transition: "opacity 160ms ease" }}
      >
        {rects}
      </g>
    );
  });
  const marks = entries.map((entry, index) => {
    const helped = (entry.subagentNew ?? 0) > 0 || Boolean(entry.subagentParts);
    const resent = entry.recached != null && entry.recached > 0;
    if (!helped && !resent) return null;
    const center = index * slot + slot / 2;
    const y = tops[index] - 11;
    const both = helped && resent;
    return (
      <g key={`m${entry.id}`} aria-hidden="true" {...labelReveal}>
        {helped && (
          <Layers
            x={center - 4.5 - (both ? 4 : 0)}
            y={y}
            size={9}
            color={theme.secondary}
          />
        )}
        {resent && (
          <RotateCw
            x={center - 4.5 + (both ? 4 : 0)}
            y={y}
            size={9}
            color={theme.warning}
            strokeWidth={2.6}
          />
        )}
      </g>
    );
  });
  const valueLabels = largest.map((index) => {
    const center = Math.min(Math.max(index * slot + slot / 2, 16), plot - 16);
    if (placed.some((x) => Math.abs(x - center) < 36)) return null;
    placed.push(center);
    const hasMark = marks[index] !== null;
    return (
      <Halo
        key={`v${index}`}
        {...labelReveal}
        theme={theme}
        x={center}
        y={Math.max(10, tops[index] - (hasMark ? 14 : 4))}
        textAnchor="middle"
        fill={theme.text}
        fontWeight={600}
        style={{ ...numeric, ...labelReveal.style }}
      >
        {measureText(values[index], measure, zh)}
      </Halo>
    );
  });
  const ticks = turnTicks(entries.length);
  const hovered = pointed !== null ? entries[pointed] : undefined;
  return (
    <div ref={ref} style={{ position: "relative", height: total, minWidth: 0 }}>
      {width > 0 && (
        <svg
          width={width}
          height={total}
          role="img"
          aria-label={
            measure === "cost"
              ? zh
                ? "每轮费用"
                : "Cost per turn"
              : zh
                ? "每轮新增 Token"
                : "New tokens per turn"
          }
          style={{
            display: "block",
            cursor: entries.length ? "pointer" : "default",
            overflow: "visible",
          }}
          onMouseMove={(event) =>
            setPointed(at(event.clientX, event.currentTarget))
          }
          onMouseLeave={() => setPointed(null)}
          onClick={(event) => {
            const index = at(event.clientX, event.currentTarget);
            if (index !== null) onSelect(entries[index]);
          }}
        >
          {picked >= 0 && (
            <rect
              x={picked * slot}
              y={0}
              width={slot}
              height={height}
              fill={theme.pick}
              rx={2}
            />
          )}
          {pointed !== null && pointed !== picked && (
            <rect
              x={pointed * slot}
              y={0}
              width={slot}
              height={height}
              fill={theme.pick}
              opacity={0.6}
              rx={2}
            />
          )}
          {scale.ticks.map((tick) => {
            const y = height - tick * yScale;
            return (
              <g key={`g${tick}`}>
                <rect
                  x={0}
                  y={y - 0.25}
                  width={plot}
                  height={0.5}
                  fill={tick === 0 ? theme.divider : theme.track}
                  opacity={tick === 0 ? 1 : 0.6}
                />
                <text
                  {...labelFont}
                  x={plot + 6}
                  y={y + 3.5}
                  fill={theme.secondary}
                  style={numeric}
                >
                  {tick === 0 ? "0" : measureText(tick, measure, zh)}
                </text>
              </g>
            );
          })}
          {entries.map((entry, index) =>
            entry.compacted ? (
              <line
                key={`k${entry.id}`}
                x1={index * slot}
                x2={index * slot}
                y1={4}
                y2={height}
                stroke={theme.compaction}
                strokeWidth={1.25}
                strokeDasharray="3 3"
              />
            ) : null,
          )}
          {bars}
          {marks}
          {valueLabels}
          {ticks.map((index) => {
            const center = index * slot + slot / 2;
            return (
              <text
                key={`x${index}`}
                {...labelFont}
                x={Math.min(Math.max(center, 10), plot - 10)}
                y={total - 4}
                textAnchor="middle"
                fill={theme.secondary}
                style={numeric}
              >
                {`T${entries[index].index}`}
              </text>
            );
          })}
        </svg>
      )}
      {hovered && pointed !== null && (
        <div
          role="tooltip"
          style={{
            position: "absolute",
            top: 0,
            ...(pointed * slot + slot / 2 > plot / 2
              ? { right: width - pointed * slot + 8 }
              : { left: (pointed + 1) * slot + 8 }),
            pointerEvents: "none",
            zIndex: 2,
            animation: "session-fade-in 140ms ease",
          }}
        >
          {hover(hovered)}
        </div>
      )}
    </div>
  );
}
