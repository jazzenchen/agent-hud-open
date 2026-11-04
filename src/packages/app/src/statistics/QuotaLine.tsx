import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  quotaDisplayLabel,
  quotaPercent,
  quotaPercentLabel,
  type QuotaDisplay,
} from "@agent-hud/ui/format";
import type { QuotaHistoryWindow } from "@agent-hud/protocol";
import { card, dateLabel, number, type StatsTheme } from "./StatsTheme";
import { useCanvas } from "./use-canvas";

const HEIGHT = 44;

function accent(theme: StatsTheme) {
  return theme.light ? "#007aff" : "#0a84ff";
}

/** One window's line on a fixed 0–100% axis: its runs, never joined across a reset or a gap, the resets as dashed rules, and the current cycle's steady budget as a faint diagonal. */
export const QuotaLine = memo(function QuotaLine({
  window: quota,
  from,
  to,
  display,
  theme,
  reveal,
  timeZone,
}: {
  window: QuotaHistoryWindow;
  from: number;
  to: number;
  display: QuotaDisplay;
  theme: StatsTheme;
  /** Changes when the stretch does: the line draws in from the left again. */
  reveal: string;
  timeZone: string;
}) {
  const { ref, width, context } = useCanvas(HEIGHT);
  const zh = Boolean(theme.zh);
  const language = zh ? "zh" : "en";
  const [hovered, setHovered] = useState<[number, number] | null>(null);
  const points = useMemo(() => quota.segments.flat(), [quota.segments]);
  const span = Math.max(1, to - from);
  const x = useCallback(
    (at: number) => ((at - from) / span) * width,
    [from, span, width],
  );
  const y = useCallback(
    (used: number) =>
      1 + (1 - (quotaPercent(used, display) ?? 0) / 100) * (HEIGHT - 2),
    [display],
  );
  const budget = useMemo(() => {
    const reset = quota.resetAt ? Date.parse(quota.resetAt) : NaN;
    const length = (quota.windowDurationSeconds ?? 0) * 1000;
    return Number.isFinite(reset) && length > 0
      ? { start: reset - length, end: reset }
      : null;
  }, [quota.resetAt, quota.windowDurationSeconds]);
  const paint = useCallback(
    (progress: number) => {
      const ctx = context();
      if (!ctx) return;
      // The axis: its top, middle and bottom.
      ctx.fillStyle = theme.divider;
      for (const level of [0, 0.5, 1])
        ctx.fillRect(0, Math.round(level * (HEIGHT - 1)), width, 1);
      ctx.save();
      ctx.beginPath();
      ctx.rect(0, 0, width * progress, HEIGHT);
      ctx.clip();
      ctx.setLineDash([2, 3]);
      ctx.lineWidth = 1;
      ctx.strokeStyle = theme.tertiary;
      for (const at of quota.resets) {
        const left = Math.round(x(at)) + 0.5;
        ctx.beginPath();
        ctx.moveTo(left, 0);
        ctx.lineTo(left, HEIGHT);
        ctx.stroke();
      }
      if (budget) {
        ctx.globalAlpha = 0.7;
        ctx.strokeStyle = theme.secondary;
        ctx.setLineDash([3, 3]);
        ctx.beginPath();
        ctx.moveTo(x(budget.start), y(0));
        ctx.lineTo(x(budget.end), y(100));
        ctx.stroke();
        ctx.globalAlpha = 1;
      }
      ctx.setLineDash([]);
      const color = accent(theme);
      for (const segment of quota.segments) {
        if (!segment.length) continue;
        if (segment.length === 1) {
          ctx.fillStyle = color;
          ctx.beginPath();
          ctx.arc(x(segment[0][0]), y(segment[0][1]), 1.5, 0, Math.PI * 2);
          ctx.fill();
          continue;
        }
        ctx.beginPath();
        segment.forEach(([at, used], index) =>
          index ? ctx.lineTo(x(at), y(used)) : ctx.moveTo(x(at), y(used)),
        );
        ctx.lineWidth = 2;
        ctx.lineJoin = "round";
        ctx.lineCap = "round";
        ctx.strokeStyle = color;
        ctx.stroke();
        ctx.lineTo(x(segment[segment.length - 1][0]), HEIGHT - 1);
        ctx.lineTo(x(segment[0][0]), HEIGHT - 1);
        ctx.closePath();
        ctx.globalAlpha = 0.12;
        ctx.fillStyle = color;
        ctx.fill();
        ctx.globalAlpha = 1;
      }
      ctx.restore();
      if (hovered) {
        const left = x(hovered[0]);
        ctx.fillStyle = theme.secondary;
        ctx.globalAlpha = 0.6;
        ctx.fillRect(Math.round(left), 0, 1, HEIGHT);
        ctx.globalAlpha = 1;
        ctx.beginPath();
        ctx.arc(left, y(hovered[1]), 3.5, 0, Math.PI * 2);
        ctx.fillStyle = color;
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = theme.ground;
        ctx.stroke();
      }
    },
    [
      budget,
      context,
      hovered,
      quota.resets,
      quota.segments,
      theme,
      width,
      x,
      y,
    ],
  );
  const revealed = useRef<string | null>(null);
  useEffect(() => {
    if (width <= 0) return;
    // A new stretch draws in from the left; new readings in the same one
    // land in place, and Reduce Motion draws it whole at once.
    if (
      revealed.current === reveal ||
      matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      revealed.current = reveal;
      paint(1);
      return;
    }
    revealed.current = reveal;
    const start = performance.now();
    let frame = 0;
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / 360);
      paint(1 - Math.pow(1 - t, 3));
      if (t < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  }, [paint, reveal, width]);
  useEffect(() => setHovered(null), [quota.segments]);
  const locate = (clientX: number) => {
    const bounds = ref.current?.getBoundingClientRect();
    if (!bounds?.width || !points.length) return;
    const at = from + ((clientX - bounds.left) / bounds.width) * span;
    let nearest = points[0];
    for (const point of points)
      if (Math.abs(point[0] - at) < Math.abs(nearest[0] - at)) nearest = point;
    // Only a reading near the pointer, so none is offered across a gap.
    const best =
      Math.abs(x(nearest[0]) - (clientX - bounds.left)) <= 24 ? nearest : null;
    setHovered((old) =>
      old && best && old[0] === best[0] && old[1] === best[1] ? old : best,
    );
  };
  const latest = points.length ? points[points.length - 1] : null;
  return (
    <div
      role="img"
      aria-label={
        latest
          ? zh
            ? `${quota.shortLabel ?? quota.name}：最近读数 ${quotaPercentLabel(latest[1], display)} ${quotaDisplayLabel(display, language)}`
            : `${quota.shortLabel ?? quota.name}: latest reading ${quotaPercentLabel(latest[1], display)} ${quotaDisplayLabel(display, language).toLowerCase()}`
          : zh
            ? `${quota.shortLabel ?? quota.name}：该范围内没有读数`
            : `${quota.shortLabel ?? quota.name}: no readings in this range`
      }
      style={{ position: "relative", height: HEIGHT }}
    >
      <canvas
        ref={ref}
        aria-hidden
        onPointerMove={(event) => locate(event.clientX)}
        onPointerLeave={() => setHovered(null)}
        style={{ width: "100%", height: HEIGHT, display: "block" }}
      />
      {!points.length && (
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
          {zh ? "该范围内没有读数" : "No readings in this range"}
        </div>
      )}
      {hovered && (
        <div
          role="tooltip"
          style={{
            ...card(theme),
            position: "absolute",
            zIndex: 10,
            pointerEvents: "none",
            bottom: HEIGHT + 6,
            left: Math.max(0, Math.min(width - 180, x(hovered[0]) - 90)),
            width: 180,
            padding: "8px 10px",
            background: theme.ground,
            boxShadow: "0 5px 18px #0002",
            fontSize: 11,
          }}
        >
          <div style={{ color: theme.secondary, marginBottom: 3 }}>
            {dateLabel(hovered[0], timeZone, zh)}
          </div>
          <strong style={{ ...number, fontSize: 13 }}>
            {quotaPercentLabel(hovered[1], display)}
          </strong>{" "}
          <span style={{ color: theme.secondary }}>
            {quotaDisplayLabel(display, language)}
          </span>
        </div>
      )}
    </div>
  );
});
