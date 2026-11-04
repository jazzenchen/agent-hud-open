import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { tokenPlot } from "../lib/statistics";
import type { StatisticsResponse } from "@agent-hud/protocol";
import {
  card,
  compact,
  dateLabel,
  integer,
  modelColor,
  number,
  row,
  truncate,
  useAnimatedNumber,
  type StatsTheme,
} from "./StatsTheme";
import { Swatch } from "./StatsParts";
import { useCanvas } from "./use-canvas";

interface BarSegment {
  id: string;
  color: string;
  ratio: number;
}

interface NormalizedBar {
  leftRatio: number;
  rightRatio: number;
  totalRatio: number;
  segments: BarSegment[];
}

function interpolateBars(
  from: NormalizedBar[],
  to: NormalizedBar[],
  t: number,
): NormalizedBar[] {
  if (t >= 1 || !from.length) return to;
  if (!to.length) {
    return from.map((bar) => ({
      ...bar,
      totalRatio: bar.totalRatio * (1 - t),
      segments: bar.segments.map((seg) => ({
        ...seg,
        ratio: seg.ratio * (1 - t),
      })),
    }));
  }
  return to.map((target, index) => {
    const source =
      from.length === to.length
        ? from[index]
        : from[
            Math.min(
              from.length - 1,
              Math.floor((index / Math.max(1, to.length)) * from.length),
            )
          ];
    if (!source) {
      return {
        ...target,
        totalRatio: target.totalRatio * t,
        segments: target.segments.map((seg) => ({
          ...seg,
          ratio: seg.ratio * t,
        })),
      };
    }
    const seen = new Set<string>();
    const segments: BarSegment[] = [];
    for (const seg of target.segments) {
      seen.add(seg.id);
      const prev = source.segments.find((s) => s.id === seg.id)?.ratio ?? 0;
      const ratio = prev + (seg.ratio - prev) * t;
      if (ratio > 0.0005) {
        segments.push({ id: seg.id, color: seg.color, ratio });
      }
    }
    for (const seg of source.segments) {
      if (seen.has(seg.id)) continue;
      const ratio = seg.ratio * (1 - t);
      if (ratio > 0.0005) {
        segments.push({ id: seg.id, color: seg.color, ratio });
      }
    }
    return {
      leftRatio: target.leftRatio,
      rightRatio: target.rightRatio,
      totalRatio:
        source.totalRatio + (target.totalRatio - source.totalRatio) * t,
      segments,
    };
  });
}

/** The original chart draws dense model stacks on one surface, rather than a view per segment. */
export const UsageChart = memo(function UsageChart({
  data,
  theme,
}: {
  data: StatisticsResponse;
  theme: StatsTheme;
}) {
  const height = 100;
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
  const targetBars = useMemo<NormalizedBar[]>(() => {
    const span = plot.end - plot.start;
    if (span <= 0 || peak <= 0) return [];
    return data.buckets.map((bucket, index) => {
      const column = plot.columns[index];
      const leftRatio = (column.start - plot.start) / span;
      const rightRatio = (column.end - plot.start) / span;
      const totalRatio = bucket.selectedTokens / peak;
      const segments: BarSegment[] = [];
      for (const model of chartModels) {
        const value = bucket.models[model.id] ?? 0;
        if (value > 0) {
          segments.push({
            id: model.id,
            color: model.color,
            ratio: value / peak,
          });
        }
      }
      return { leftRatio, rightRatio, totalRatio, segments };
    });
  }, [chartModels, data.buckets, peak, plot]);

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
    if (
      prev === null ||
      matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      currentBarsRef.current = targetBars;
      paint(targetBars, hoveredRef.current);
      return;
    }
    const from = prev;
    const duration = 240;
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
          <div
            role="tooltip"
            style={{
              ...card(theme),
              position: "absolute",
              zIndex: 10,
              pointerEvents: "none",
              bottom: 108,
              left: Math.max(
                0,
                Math.min(
                  width - Math.min(width, 280),
                  ((inspectedColumn.start - plot.start) /
                    (plot.end - plot.start)) *
                    width -
                    130,
                ),
              ),
              width: Math.min(width, 280),
              background: theme.ground,
              boxShadow: "0 5px 18px #0002",
              fontSize: 11,
            }}
          >
            <div style={{ color: theme.secondary, marginBottom: 5 }}>
              {dateLabel(
                Math.max(inspectedColumn.start, start),
                data.query.timeZone,
                zh,
              )}
            </div>
            <strong style={{ ...number, fontSize: 14 }}>
              {integer(inspected.selectedTokens)} {zh ? "Token" : "tokens"}
            </strong>
            {data.models
              .filter((model) => (inspected.models[model.id] ?? 0) > 0)
              .map((model) => (
                <div key={model.id} style={{ ...row, marginTop: 5 }}>
                  <Swatch color={modelColor(model.id)} />
                  <HUDLogo
                    source={model.provider}
                    size={11}
                    light={theme.light}
                  />
                  <span style={truncate}>{model.name}</span>
                  <span style={{ marginLeft: "auto", ...number }}>
                    {integer(inspected.models[model.id])}
                  </span>
                </div>
              ))}
            {inspectedColumn.partial && (
              <div style={{ color: theme.secondary, marginTop: 5 }}>
                {zh
                  ? "仅统计所选范围内的部分时段"
                  : "Partial period within the selected range"}
              </div>
            )}
          </div>
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

export const Sparkline = memo(function Sparkline({
  values,
  color,
}: {
  values: number[];
  color: string;
}) {
  const height = 28;
  const { ref, width, context } = useCanvas(height);
  const targetRatios = useMemo(() => {
    const peak = Math.max(0, ...values);
    return peak > 0
      ? values.map((v) => Math.max(0, v / peak))
      : values.map(() => 0);
  }, [values]);
  const currentRatiosRef = useRef<number[] | null>(null);

  const paint = useCallback(
    (ratios: number[]) => {
      const ctx = context();
      if (!ctx || !ratios.length) return;
      const slot = width / ratios.length;
      ctx.fillStyle = color;
      ctx.globalAlpha = 0.85;
      for (const [index, ratio] of ratios.entries()) {
        if (ratio > 0.001) {
          const h = Math.max(1.5, height * ratio);
          ctx.fillRect(
            index * slot + slot * 0.2,
            height - h,
            Math.max(1, slot * 0.6),
            h,
          );
        }
      }
    },
    [color, context, width],
  );

  useEffect(() => {
    if (!width) return;
    const prev = currentRatiosRef.current;
    if (
      prev === null ||
      matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      currentRatiosRef.current = targetRatios;
      paint(targetRatios);
      return;
    }
    const from = prev;
    const duration = 240;
    const startTime = performance.now();
    let frame = 0;
    const animate = (now: number) => {
      const t = Math.min(1, Math.max(0, (now - startTime) / duration));
      const eased = 1 - Math.pow(1 - t, 3);
      const next = targetRatios.map((target, idx) => {
        const src =
          from.length === targetRatios.length
            ? (from[idx] ?? 0)
            : (from[
                Math.min(
                  from.length - 1,
                  Math.floor(
                    (idx / Math.max(1, targetRatios.length)) * from.length,
                  ),
                )
              ] ?? 0);
        return src + (target - src) * eased;
      });
      currentRatiosRef.current = next;
      paint(next);
      if (t < 1) {
        frame = requestAnimationFrame(animate);
      }
    };
    frame = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frame);
  }, [targetRatios, paint, width]);

  return (
    <canvas
      ref={ref}
      aria-hidden
      style={{ width: "100%", height, display: "block" }}
    />
  );
});
