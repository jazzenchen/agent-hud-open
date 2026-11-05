import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  quotaDisplayLabel,
  quotaPercent,
  quotaPercentLabel,
  type QuotaDisplay,
} from "@agent-hud/ui/format";
import type { QuotaHistoryWindow } from "@agent-hud/protocol";
import type { StatsTheme } from "./StatsTheme";
import { QuotaChartTip } from "./QuotaChartTip";
import { useCanvas } from "./use-canvas";
import { readingsAt } from "@agent-hud/hud/panel/quota-groups";

const label = (window: QuotaHistoryWindow) => window.shortLabel ?? window.name;

/** One agent's windows on a fixed 0–100% axis, each line in its colour: its
 * runs, never joined across a reset or a gap. A chart of one window, or the
 * window `focus` names, shows its resets as dashed rules and the current
 * cycle's steady budget as a faint diagonal; the others mark their resets
 * with a tick at the foot. Pointing at it lists every window's reading there. */
export const QuotaChart = memo(function QuotaChart({
  windows,
  colors,
  focus,
  from,
  to,
  display,
  theme,
  reveal,
  timeZone,
}: {
  windows: QuotaHistoryWindow[];
  colors: string[];
  /** The window a pointer is on in the legend, by its place. */
  focus: number | null;
  from: number;
  to: number;
  display: QuotaDisplay;
  theme: StatsTheme;
  /** Changes when the stretch does: the lines draw in from the left again. */
  reveal: string;
  timeZone: string;
}) {
  const height = windows.length > 1 ? 64 : 44;
  const { ref, width, context } = useCanvas(height);
  const zh = Boolean(theme.zh);
  const language = zh ? "zh" : "en";
  const [pointer, setPointer] = useState<number | null>(null);
  const span = Math.max(1, to - from);
  const x = useCallback(
    (at: number) => ((at - from) / span) * width,
    [from, span, width],
  );
  const y = useCallback(
    (used: number) =>
      1 + (1 - (quotaPercent(used, display) ?? 0) / 100) * (height - 2),
    [display, height],
  );
  const single = windows.length === 1 ? 0 : null;
  const lead = focus ?? single;
  const hovered = useMemo(
    () =>
      pointer == null || width <= 0
        ? null
        : readingsAt(windows, pointer, (24 / width) * span),
    [pointer, span, width, windows],
  );
  const paint = useCallback(
    (progress: number) => {
      const ctx = context();
      if (!ctx) return;
      ctx.fillStyle = theme.divider;
      for (const level of [0, 0.5, 1])
        ctx.fillRect(0, Math.round(level * (height - 1)), width, 1);
      ctx.save();
      ctx.beginPath();
      ctx.rect(0, 0, width * progress, height);
      ctx.clip();
      windows.forEach((window, index) => {
        const color = colors[index];
        const led = lead === index;
        ctx.lineWidth = 1;
        if (led) {
          ctx.setLineDash([2, 3]);
          ctx.strokeStyle = theme.tertiary;
          for (const at of window.resets) {
            const left = Math.round(x(at)) + 0.5;
            ctx.beginPath();
            ctx.moveTo(left, 0);
            ctx.lineTo(left, height);
            ctx.stroke();
          }
          const reset = window.resetAt ? Date.parse(window.resetAt) : NaN;
          const length = (window.windowDurationSeconds ?? 0) * 1000;
          if (Number.isFinite(reset) && length > 0) {
            ctx.globalAlpha = 0.7;
            ctx.strokeStyle = theme.secondary;
            ctx.setLineDash([3, 3]);
            ctx.beginPath();
            ctx.moveTo(x(reset - length), y(0));
            ctx.lineTo(x(reset), y(100));
            ctx.stroke();
            ctx.globalAlpha = 1;
          }
          ctx.setLineDash([]);
        } else {
          ctx.fillStyle = color;
          for (const at of window.resets)
            ctx.fillRect(Math.round(x(at)), height - 4, 1, 4);
        }
      });
      // The window in focus is drawn last, over the others.
      const order = windows.map((_, index) => index);
      if (lead != null) order.push(...order.splice(lead, 1));
      for (const index of order) {
        const color = colors[index];
        const dim = focus != null && focus !== index;
        for (const segment of windows[index].segments) {
          if (!segment.length) continue;
          ctx.globalAlpha = dim ? 0.25 : 1;
          ctx.fillStyle = color;
          if (segment.length === 1) {
            ctx.beginPath();
            ctx.arc(x(segment[0][0]), y(segment[0][1]), 1.5, 0, Math.PI * 2);
            ctx.fill();
            continue;
          }
          ctx.beginPath();
          segment.forEach(([at, used], step) =>
            step ? ctx.lineTo(x(at), y(used)) : ctx.moveTo(x(at), y(used)),
          );
          ctx.lineWidth = 2;
          ctx.lineJoin = "round";
          ctx.lineCap = "round";
          ctx.strokeStyle = color;
          ctx.stroke();
          if (lead !== index) continue;
          ctx.lineTo(x(segment[segment.length - 1][0]), height - 1);
          ctx.lineTo(x(segment[0][0]), height - 1);
          ctx.closePath();
          ctx.globalAlpha = 0.12;
          ctx.fill();
        }
        ctx.globalAlpha = 1;
      }
      ctx.restore();
      if (pointer == null || !hovered?.some(Boolean)) return;
      ctx.fillStyle = theme.secondary;
      ctx.globalAlpha = 0.6;
      ctx.fillRect(Math.round(x(pointer)), 0, 1, height);
      ctx.globalAlpha = 1;
      hovered.forEach((reading, index) => {
        if (!reading) return;
        ctx.beginPath();
        ctx.arc(x(reading[0]), y(reading[1]), 3.5, 0, Math.PI * 2);
        ctx.fillStyle = colors[index];
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = theme.ground;
        ctx.stroke();
      });
    },
    [
      colors,
      context,
      focus,
      height,
      hovered,
      lead,
      pointer,
      theme,
      width,
      windows,
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
  useEffect(() => setPointer(null), [windows]);
  const empty = windows.every(
    (window) => !window.segments.some((run) => run.length),
  );
  const latest = windows
    .map((window) => {
      const points = window.segments.flat();
      const last = points[points.length - 1];
      return last
        ? `${label(window)} ${quotaPercentLabel(last[1], display)}`
        : null;
    })
    .filter(Boolean)
    .join(", ");
  const shown = hovered?.flatMap((reading, index) =>
    reading ? [{ reading, index }] : [],
  );
  return (
    <div
      role="img"
      aria-label={
        empty
          ? zh
            ? "该范围内没有读数"
            : "No readings in this range"
          : `${quotaDisplayLabel(display, language)}: ${latest}`
      }
      style={{ position: "relative", height }}
    >
      <canvas
        ref={ref}
        aria-hidden
        onPointerMove={(event) => {
          const bounds = ref.current?.getBoundingClientRect();
          if (bounds?.width)
            setPointer(
              from + ((event.clientX - bounds.left) / bounds.width) * span,
            );
        }}
        onPointerLeave={() => setPointer(null)}
        style={{ width: "100%", height, display: "block" }}
      />
      {empty && (
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
      {pointer != null && shown?.length ? (
        <QuotaChartTip
          shown={shown.map(({ reading, index }) => ({
            reading,
            window: windows[index],
            color: colors[index],
          }))}
          left={Math.max(0, Math.min(width - 200, x(pointer) - 100))}
          bottom={height + 6}
          display={display}
          theme={theme}
          timeZone={timeZone}
        />
      ) : null}
    </div>
  );
});
