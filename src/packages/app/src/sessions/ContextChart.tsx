import { contextRuns, niceScale } from "../lib/session-charts";
import { axis, Halo, labelFont, useWidth } from "./SessionCharts";
import { compact, numeric, type Entry, type Theme } from "./SessionTheme";
import { useGlide } from "./use-glide";

/**
 * Each turn's context against the model's window, on the turn chart's axis
 * (the phone's `ContextWindowChart`): a line and its area, broken where the
 * client compacted, each compaction a labelled amber line; a whisker up to a
 * turn's peak when it ended lower; the window as a labelled dashed line over
 * a near-limit band; the last point named with its tokens and fill. The line
 * rises from the base when first drawn and eases to new values after.
 */
export function ContextChart({
  entries,
  window,
  fill,
  selected,
  theme,
  height: total = 104,
}: {
  entries: Entry[];
  window: number | null | undefined;
  /** Core's fill of the latest call, for the last point's label. */
  fill: number | null | undefined;
  selected: string | null;
  theme: Theme;
  height?: number;
}) {
  const zh = Boolean(theme.zh);
  const [ref, width] = useWidth<HTMLDivElement>();
  const plot = Math.max(0, width - axis);
  const pad = 6;
  const height = total - 2;
  const peak = Math.max(
    0,
    ...entries.map((entry) => entry.peakContext ?? entry.context ?? 0),
  );
  const limit = window != null && window > 0 ? window : null;
  const top = limit ?? niceScale(peak, 2).top;
  const y = (tokens: number) =>
    height - (Math.min(tokens, top) / top) * (height - pad);
  // What the line draws, easing: each turn's context and peak, the scale.
  const contexts = useGlide(entries.map((entry) => entry.context ?? 0));
  const peaks = useGlide(
    entries.map((entry) => entry.peakContext ?? entry.context ?? 0),
  );
  const [scaleTop, lastFill] = useGlide([top, fill ?? 0], [top, 0]);
  const at = (tokens: number) =>
    height -
    (Math.min(tokens, scaleTop) / Math.max(1, scaleTop)) * (height - pad);
  const slot = entries.length > 0 ? plot / entries.length : plot;
  const center = (index: number) => index * slot + slot / 2;
  const runs = contextRuns(entries);
  const picked = entries.findIndex((entry) => entry.id === selected);
  const color = theme.cache;
  const lastIndex = runs.at(-1)?.at(-1);
  const last = lastIndex !== undefined ? entries[lastIndex] : undefined;
  return (
    <div ref={ref} style={{ height: total, minWidth: 0 }}>
      {width > 0 && (
        <svg
          width={width}
          height={total}
          role="img"
          aria-label={zh ? "上下文窗口" : "Context window"}
          style={{ display: "block", overflow: "visible" }}
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
          {limit !== null && (
            <>
              <rect
                x={0}
                y={y(top)}
                width={plot}
                height={y(top * 0.8) - y(top)}
                fill={theme.warning}
                opacity={0.09}
              />
              <line
                x1={0}
                x2={plot}
                y1={y(top)}
                y2={y(top)}
                stroke={theme.warning}
                strokeWidth={1}
                strokeDasharray="3 3"
              />
              <Halo
                theme={theme}
                x={4}
                y={y(top) + 12}
                fill={theme.warning}
                fontWeight={600}
              >
                {(zh ? "上限 " : "Limit ") + compact(limit)}
              </Halo>
            </>
          )}
          <rect
            x={0}
            y={y(top / 2) - 0.25}
            width={plot}
            height={0.5}
            fill={theme.track}
            opacity={0.6}
          />
          <rect
            x={0}
            y={height - 0.25}
            width={plot}
            height={0.5}
            fill={theme.divider}
          />
          {[top, top / 2, 0].map((tick) => (
            <text
              key={`y${tick}`}
              {...labelFont}
              x={plot + 6}
              y={tick === top ? y(tick) + 7 : y(tick) + 3.5}
              fill={
                tick === top && limit !== null ? theme.warning : theme.secondary
              }
              style={numeric}
            >
              {tick === 0 ? "0" : compact(tick)}
            </text>
          ))}
          {runs.map((run, index) => {
            const points = run.map(
              (position) => [center(position), at(contexts[position])] as const,
            );
            const line = points
              .map(([px, py], at) => `${at ? "L" : "M"}${px},${py}`)
              .join("");
            const area = `M${points[0][0]},${height}${points.map(([px, py]) => `L${px},${py}`).join("")}L${points.at(-1)![0]},${height}Z`;
            return (
              <g key={`r${index}`}>
                <path d={area} fill={color} fillOpacity={0.2} />
                <path
                  d={line}
                  fill="none"
                  stroke={color}
                  strokeWidth={1.5}
                  strokeLinejoin="round"
                />
              </g>
            );
          })}
          {entries.map((entry, index) =>
            entry.peakContext != null && entry.context != null ? (
              <path
                key={`p${entry.id}`}
                d={`M${center(index)},${at(contexts[index])}L${center(index)},${at(peaks[index])}M${center(index) - 2.5},${at(peaks[index])}L${center(index) + 2.5},${at(peaks[index])}`}
                stroke={color}
                strokeWidth={1}
                fill="none"
              />
            ) : null,
          )}
          {entries.map((entry, index) => {
            if (!entry.compacted) return null;
            const x = index * slot;
            const right = x > plot - 64;
            return (
              <g key={`c${entry.id}`}>
                <line
                  x1={x}
                  x2={x}
                  y1={pad}
                  y2={height}
                  stroke={theme.compaction}
                  strokeWidth={1.25}
                  strokeDasharray="3 3"
                />
                <Halo
                  theme={theme}
                  x={right ? x - 4 : x + 4}
                  y={height - 6}
                  textAnchor={right ? "end" : "start"}
                  fill={theme.compaction}
                  fontWeight={600}
                >
                  {zh ? "已压缩" : "Compacted"}
                </Halo>
              </g>
            );
          })}
          {last && lastIndex !== undefined && last.context != null && (
            <g>
              <circle
                cx={center(lastIndex)}
                cy={at(contexts[lastIndex])}
                r={3}
                fill={color}
                stroke={theme.surface}
                strokeWidth={1.5}
              />
              <Halo
                theme={theme}
                x={center(lastIndex) - 7}
                y={Math.max(12, at(contexts[lastIndex]) - 6)}
                textAnchor="end"
                fill={theme.text}
                fontWeight={600}
                style={numeric}
              >
                {compact(Math.round(contexts[lastIndex])) +
                  (fill != null ? ` · ${Math.round(lastFill * 100)}%` : "")}
              </Halo>
            </g>
          )}
        </svg>
      )}
    </div>
  );
}
