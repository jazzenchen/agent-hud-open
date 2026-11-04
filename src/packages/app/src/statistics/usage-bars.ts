import type { StatisticsBucket } from "@agent-hud/protocol";
import type { TokenPlot } from "../lib/statistics";

interface BarSegment {
  id: string;
  color: string;
  ratio: number;
}

export interface NormalizedBar {
  leftRatio: number;
  rightRatio: number;
  totalRatio: number;
  segments: BarSegment[];
}

export function interpolateBars(
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

/** Each bucket's bar: where its column lies on the plot, 0…1 from the left
 * edge, and its stack of models, as shares of the peak. */
export function usageBars(
  buckets: StatisticsBucket[],
  plot: TokenPlot,
  peak: number,
  chartModels: { id: string; color: string }[],
): NormalizedBar[] {
  const span = plot.end - plot.start;
  if (span <= 0 || peak <= 0) return [];
  return buckets.map((bucket, index) => {
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
}
