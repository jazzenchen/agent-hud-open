import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { tokenPlot } from "../lib/statistics";
import type { StatisticsResponse } from "@agent-hud/protocol";
import {
  compact,
  dateLabel,
  integer,
  modelColor,
  number,
  useAnimatedNumber,
  type StatsTheme,
} from "./StatsTheme";
import { useCanvas } from "./use-canvas";
import { interpolateBars, usageBars, type NormalizedBar } from "./usage-bars";
import { UsageChartTip } from "./UsageChartTip";

/** The original chart draws dense model stacks on one surface, rather than a view per segment. */
export const UsageChart = memo(function UsageChart({
  data,
  theme,
  height = 100,
}: {
  data: StatisticsResponse;
  theme: StatsTheme;
  /** The plot's height, points; the Usage page draws it taller. */
  height?: number;
}) {
  const { ref, width, context } = useCanvas(height);
  const [hovered, setHovered] = useState<number | null>(null);
  const start = Date.parse(data.query.from),
    end = Date.parse(data.query.to);
  // Whole buckets at both ends, as Open's plotInterval: a bucket the range cuts
  // is drawn full width, and its count still covers only the range.
  const plot = useMemo(
    () => tokenPlot(data.buckets, data.query),
    [data.buckets, data.query],
  );
  const peak = Math.max(
    0,
    ...data.buckets.map((bucket) => bucket.selectedTokens),
  );
  const animatedPeak = useAnimatedNumber(peak);
  const chartModels = useMemo(
    () =>
      data.models.map((model) => ({
        id: model.id,
        color: modelColor(model.id),
      })),
    [data.models],
  );
  const targetBars = useMemo(
    () => usageBars(data.buckets, plot, peak, chartModels),
    [chartModels, data.buckets, peak, plot],
  );

  const currentBarsRef = useRef<NormalizedBar[] | null>(null);
  const hoveredRef = useRef<number | null>(hovered);
  hoveredRef.current = hovered;

  const paint = useCallback(
    (bars: NormalizedBar[], activeHover: number | null) => {
      const ctx = context();
      if (!ctx || !bars.length) return;
      if (activeHover !== null && bars[activeHover]) {
        const left = bars[activeHover].leftRatio * width;
        const right = bars[activeHover].rightRatio * width;
        ctx.save();
        ctx.fillStyle = theme.text;
        ctx.globalAlpha = 0.08;
        ctx.fillRect(left, 0, right - left, height - 1);
        ctx.fillStyle = theme.secondary;
        ctx.globalAlpha = 0.5;
        ctx.fillRect((left + right) / 2, 0, 1, height - 1);
        ctx.restore();
      }
      for (const [index, bar] of bars.entries()) {
        const left = bar.leftRatio * width;
        const right = bar.rightRatio * width;
        const gap = Math.min(2, Math.max(0, right - left) * 0.2),
          barWidth = Math.max(0, right - left - gap);
        if (!barWidth || bar.totalRatio <= 0.0005) continue;
        ctx.save();
        ctx.globalAlpha =
          activeHover === null || activeHover === index ? 1 : 0.55;
        const stackHeight = bar.totalRatio * (height - 2);
        ctx.beginPath();
        if (typeof ctx.roundRect === "function")
          ctx.roundRect(
            left + gap / 2,
            height - 1 - stackHeight,
            barWidth,
            stackHeight,
            2,
          );
        else
          ctx.rect(
            left + gap / 2,
            height - 1 - stackHeight,
            barWidth,
            stackHeight,
          );
        ctx.clip();
        let bottom = height - 1;
        for (const segment of bar.segments) {
          if (segment.ratio <= 0) continue;
          const segHeight = segment.ratio * (height - 2);
          bottom -= segHeight;
          ctx.fillStyle = segment.color;
          ctx.fillRect(left + gap / 2, bottom, barWidth, segHeight + 0.5);
        }
        ctx.restore();
      }
    },
    [context, height, theme.secondary, theme.text, width],
  );

  useEffect(() => {
    if (width <= 0) return;
    const prev = currentBarsRef.current;
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) {
      currentBarsRef.current = targetBars;
      paint(targetBars, hoveredRef.current);
      return;
    }
    // The first bars grow from the baseline; later ones move from where
    // they stood.
    const from =
      prev ??
      targetBars.map((bar) => ({
        ...bar,
        totalRatio: 0,
        segments: bar.segments.map((segment) => ({ ...segment, ratio: 0 })),
      }));
    const duration = prev ? 320 : 480;
    const startTime = performance.now();
    let frame = 0;
    const animate = (now: number) => {
      const t = Math.min(1, Math.max(0, (now - startTime) / duration));
      const eased = 1 - Math.pow(1 - t, 3);
      const interpolated = interpolateBars(from, targetBars, eased);
      currentBarsRef.current = interpolated;
      paint(interpolated, hoveredRef.current);
      if (t < 1) {
        frame = requestAnimationFrame(animate);
      }
    };
    frame = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frame);
  }, [targetBars, paint, width]);

  useEffect(() => {
    if (width <= 0 || !currentBarsRef.current) return;
    paint(currentBarsRef.current, hovered);
  }, [hovered, paint, width]);

  useEffect(() => setHovered(null), [data]);
  const zh = Boolean(theme.zh);
  const inspected = hovered === null ? null : data.buckets[hovered];
  const inspectedColumn = hovered === null ? null : plot.columns[hovered];
  const locate = (clientX: number) => {
    const bounds = ref.current?.getBoundingClientRect();
    if (!bounds?.width || !data.buckets.length) return;
    const at =
      plot.start +
      Math.max(0, Math.min(1, (clientX - bounds.left) / bounds.width)) *
        (plot.end - plot.start);
    let lo = 0,
      hi = data.buckets.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (Date.parse(data.buckets[mid].timestamp) <= at) lo = mid + 1;
      else hi = mid;
    }
    const index = Math.max(0, lo - 1);
    setHovered((old) => (old === index ? old : index));
  };
  return (
    <div style={{ padding: "18px 0 0" }}>
      <div
        role="img"
        tabIndex={0}
        aria-label={
          zh
            ? `已记录 ${integer(data.totals.selectedTokens)} Token。方向键可查看各时间段。`
            : `${integer(data.totals.selectedTokens)} recorded tokens. Arrow keys inspect time buckets.`
        }
        onKeyDown={(event) => {
          if (event.key === "Escape") setHovered(null);
          if (
            (event.key === "ArrowLeft" || event.key === "ArrowRight") &&
            data.buckets.length
          ) {
            event.preventDefault();
            setHovered((old) =>
              Math.max(
                0,
                Math.min(
                  data.buckets.length - 1,
                  (old ?? 0) + (event.key === "ArrowLeft" ? -1 : 1),
                ),
              ),
            );
          }
        }}
        style={{
          height,
          position: "relative",
          borderBottom: `1px solid ${theme.divider}`,
          outlineOffset: 4,
        }}
      >
        <canvas
          ref={ref}
          aria-hidden
          onPointerMove={(event) => locate(event.clientX)}
          onPointerLeave={() => setHovered(null)}
          style={{ width: "100%", height, display: "block" }}
        />
        <span
          style={{
            position: "absolute",
            top: -15,
            right: 0,
            color: theme.secondary,
            fontSize: 10,
            pointerEvents: "none",
            ...number,
          }}
        >
          {compact(animatedPeak)}
        </span>
        {peak === 0 && (
          <div
            style={{
              position: "absolute",
              inset: 0,
              display: "grid",
              placeItems: "center",
              pointerEvents: "none",
              color: theme.secondary,
              fontSize: 11,
            }}
          >
            {data.coverage.state === "unavailable"
              ? zh
                ? "当前筛选暂无用量数据"
                : "Usage is unavailable for this selection"
              : data.query.models?.length === 0 ||
                  data.query.providers?.length === 0
                ? zh
                  ? "未选择模型或智能体"
                  : "No models or agents selected"
                : zh
                  ? "该时间范围内无已记录的 Token 用量"
                  : "No recorded token usage in this range"}
          </div>
        )}
        {inspected && inspectedColumn && (
          <UsageChartTip
            data={data}
            inspected={inspected}
            inspectedColumn={inspectedColumn}
            plot={plot}
            start={start}
            width={width}
            theme={theme}
          />
        )}
      </div>
      {/* The labels name the selected range and sit where it lies on the
          plot, a part of a bucket in from either edge. */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          paddingTop: 7,
          paddingLeft: `${plot.from * 100}%`,
          paddingRight: `${(1 - plot.to) * 100}%`,
          color: theme.secondary,
          fontSize: 10,
        }}
      >
        {[0, 1 / 3, 2 / 3, 1].map((part) => (
          <span key={part} style={{ maxWidth: "26%" }}>
            {dateLabel(start + (end - start) * part, data.query.timeZone, zh)}
          </span>
        ))}
      </div>
    </div>
  );
});
