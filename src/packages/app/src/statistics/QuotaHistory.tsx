import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useCoreQuery } from "../lib/use-core-query";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { useFormat } from "@agent-hud/ui/format-context";
import {
  budgetText,
  outlookBasisText,
  outlookText,
  quotaDisplayLabel,
  quotaPercent,
  quotaPercentLabel,
  type QuotaDisplay,
} from "@agent-hud/ui/format";
import type {
  NativeMethod,
  Quota,
  QuotaHistoryResponse,
  QuotaHistoryWindow,
} from "@agent-hud/protocol";
import {
  card,
  dateLabel,
  number,
  row,
  truncate,
  useCanvasWidth,
  type StatsTheme,
} from "./StatsTheme";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

const HEIGHT = 44;

function accent(theme: StatsTheme) {
  return theme.light ? "#007aff" : "#0a84ff";
}

/** One window's line on a fixed 0–100% axis: its runs, never joined across a reset or a gap, the resets as dashed rules, and the current cycle's steady budget as a faint diagonal. */
const QuotaLine = memo(function QuotaLine({
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
  const { ref, width } = useCanvasWidth();
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
      const canvas = ref.current,
        ctx = canvas?.getContext("2d");
      if (!canvas || !ctx || width <= 0) return;
      const scale = window.devicePixelRatio || 1;
      const targetW = Math.round(width * scale),
        targetH = Math.round(HEIGHT * scale);
      if (canvas.width !== targetW || canvas.height !== targetH) {
        canvas.width = targetW;
        canvas.height = targetH;
      }
      ctx.setTransform(scale, 0, 0, scale, 0, 0);
      ctx.clearRect(0, 0, width, HEIGHT);
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
    [budget, hovered, quota.resets, quota.segments, ref, theme, width, x, y],
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

/** What the window reads now, from the snapshot Core worked out: its share, its budget, and its forecast with the pace it comes from. */
function nowLine(
  quota: Quota | undefined,
  display: QuotaDisplay,
  now: number,
  zh: boolean,
) {
  if (!quota || quota.usedPercent == null || quota.status === "stale")
    return null;
  const language = zh ? "zh" : "en";
  const parts = [quotaPercentLabel(quota.usedPercent, display)];
  const metrics = quota.metrics;
  if (metrics?.budget && Number.isFinite(metrics.budget.paceDelta))
    parts.push(budgetText(metrics.budget, language));
  const outlook = metrics?.outlook;
  if (outlook) {
    const exhaustsAt = outlook.exhaustsAt
      ? Date.parse(outlook.exhaustsAt)
      : NaN;
    const words = outlookText(
      {
        kind: outlook.kind,
        seconds: Number.isFinite(exhaustsAt)
          ? Math.max(0, (exhaustsAt - now) / 1000)
          : undefined,
        beforeReset: outlook.beforeReset,
      },
      outlook.projectedUsedPercent ?? null,
      language,
    );
    if (words && ["exhausted", "exhausts", "no-usage"].includes(outlook.kind)) {
      parts.push(words);
      if (outlook.basis) parts.push(outlookBasisText(outlook.basis, language));
    }
  }
  return parts.join(" · ");
}

/** The Statistics window's quota history: one row per shown window over the selected range, each on its own fixed 0–100% axis. */
export function QuotaHistory({
  quotas,
  from,
  to,
  action,
  active,
  theme,
  light,
  timeZone,
}: {
  quotas: Quota[];
  from: number;
  to: number;
  action: Action;
  active: boolean;
  theme: StatsTheme;
  light: boolean;
  timeZone: string;
}) {
  // Only the latest stretch publishes; a failure keeps the last one drawn.
  const { data, error } = useCoreQuery<QuotaHistoryResponse>(
    action,
    "quota_history",
    { from: new Date(from).toISOString(), to: new Date(to).toISOString() },
    active,
  );
  const display = useFormat().quotaDisplay ?? "consumed";
  const zh = Boolean(theme.zh);
  const now = Date.now();
  const live = useMemo(
    () => new Map(quotas.map((quota) => [quota.id, quota])),
    [quotas],
  );
  const shownFrom = data ? Date.parse(data.from) : from;
  const shownTo = data ? Date.parse(data.to) : to;
  // A stretch of another length, once it has come, draws the lines in again.
  const reveal = String(shownTo - shownFrom);
  const accounts = useMemo(() => {
    const ids = new Map<string, Set<string>>();
    for (const quota of data?.windows ?? []) {
      const vendor = ids.get(quota.vendor) ?? new Set<string>();
      vendor.add(quota.accountId ?? "");
      ids.set(quota.vendor, vendor);
    }
    return ids;
  }, [data]);
  return (
    <div
      className="statistics-quota-history"
      style={{ padding: "6px 16px 8px", minWidth: 0 }}
    >
      <div
        style={{
          ...row,
          alignItems: "baseline",
          justifyContent: "space-between",
          marginBottom: 6,
        }}
      >
        <strong style={{ fontSize: 13, fontWeight: 600 }}>
          {zh ? "额度历史" : "Quota history"}
        </strong>
        <span style={{ color: theme.secondary, fontSize: 11 }}>
          {quotaDisplayLabel(display, zh ? "zh" : "en")} · 0–100%
        </span>
      </div>
      {error && !data && (
        <div role="alert" style={{ color: theme.secondary, fontSize: 11 }}>
          {zh ? "额度历史暂不可用：" : "Quota history is unavailable: "}
          {error}
        </div>
      )}
      {!data && !error && (
        <div style={{ color: theme.secondary, fontSize: 11, padding: "8px 0" }}>
          {zh ? "正在加载额度历史…" : "Loading quota history…"}
        </div>
      )}
      {data && !data.windows.length && (
        <div style={{ color: theme.secondary, fontSize: 11, padding: "8px 0" }}>
          {zh ? "暂无可显示的额度窗口" : "No quota windows to show"}
        </div>
      )}
      <div style={{ display: "grid", gap: 14 }}>
        {data?.windows.map((quota) => {
          const reading = nowLine(live.get(quota.id), display, now, zh);
          const several = (accounts.get(quota.vendor)?.size ?? 0) > 1;
          return (
            <div key={quota.id} style={{ display: "grid", gap: 5 }}>
              <div style={{ ...row, fontSize: 11 }}>
                <HUDLogo source={quota.vendor} size={12} light={light} />
                <span
                  style={{ ...truncate, fontWeight: 600 }}
                  title={quota.name}
                >
                  {quota.shortLabel ?? quota.name}
                </span>
                {several && quota.account && (
                  <span style={{ ...truncate, color: theme.secondary }}>
                    {quota.account}
                  </span>
                )}
                {reading && (
                  <span
                    title={reading}
                    style={{
                      ...truncate,
                      ...number,
                      marginLeft: "auto",
                      color: theme.secondary,
                    }}
                  >
                    {reading}
                  </span>
                )}
              </div>
              <QuotaLine
                window={quota}
                from={shownFrom}
                to={shownTo}
                display={display}
                theme={theme}
                reveal={reveal}
                timeZone={timeZone}
              />
            </div>
          );
        })}
      </div>
      {data && data.windows.length > 0 && (
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            paddingTop: 7,
            color: theme.secondary,
            fontSize: 10,
          }}
        >
          {[0, 1 / 3, 2 / 3, 1].map((part) => (
            <span key={part} style={{ maxWidth: "26%" }}>
              {dateLabel(
                shownFrom + (shownTo - shownFrom) * part,
                timeZone,
                zh,
              )}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
