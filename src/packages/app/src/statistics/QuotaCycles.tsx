import { useEffect, useRef, useState } from "react";
import type { QuotaCycle, QuotaHistoryWindow } from "@agent-hud/protocol";
import { cycleLevel } from "@agent-hud/hud/panel/quota-groups";
import { card, number, type StatsTheme } from "./StatsTheme";

/** The bars' height at 100%, points. */
export const CYCLE_HEIGHT = 40;

const percent = (value: number) => `${Math.round(value)}%`;

/** A share's colour: quiet while it is fine, amber from 70%, red from 90%. */
export function levelFill(used: number, theme: StatsTheme): string {
  const level = cycleLevel(used);
  if (level === "critical") return theme.light ? "#e0352b" : "#ff453a";
  if (level === "warning") return theme.light ? "#c27c00" : "#ff9f0a";
  return theme.secondary;
}

function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const observer = new ResizeObserver(([entry]) =>
      setWidth(Math.round(entry.contentRect.width)),
    );
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return { ref, width };
}

/** One window's cycles over the stretch `from`–`until`, each as wide as the
 * cycle and as high as the share it used, on a fixed 0–100% scale. A
 * five-hour cycle is a bar; a week is a faint band with a line at its
 * height, so a long cycle does not drown the short ones. The running cycle
 * is drawn lighter, with a dotted line where Core's forecast puts it at its
 * reset. Grey while fine; amber and red only where a cycle ran high. */
export function QuotaCycles({
  window,
  from,
  until,
  now,
  projected,
  theme,
  timeZone,
}: {
  window: QuotaHistoryWindow;
  from: number;
  until: number;
  now: number;
  /** Core's forecast for the running cycle at its reset. */
  projected: number | null;
  theme: StatsTheme;
  timeZone: string;
}) {
  const zh = Boolean(theme.zh);
  const { ref, width } = useWidth();
  const [hovered, setHovered] = useState<QuotaCycle | null>(null);
  const [pointer, setPointer] = useState(0);
  const span = Math.max(1, until - from);
  const x = (at: number) =>
    ((Math.min(Math.max(at, from), until) - from) / span) * 100;
  const long = (window.windowDurationSeconds ?? 0) >= 86_400;
  const time = new Intl.DateTimeFormat(zh ? "zh-CN" : "en-US", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone,
  });
  const H = CYCLE_HEIGHT;
  return (
    <div
      ref={ref}
      onPointerLeave={() => setHovered(null)}
      style={{
        position: "relative",
        height: H + 12,
        boxShadow: `inset 0 -1px 0 ${theme.divider}`,
      }}
    >
      {(window.cycles ?? []).map((cycle) => {
        const left = x(cycle.start);
        const share = Math.max(0.15, x(cycle.end) - left - (long ? 0.25 : 0.1));
        const height =
          cycle.used > 0 ? Math.max(1.5, (cycle.used / 100) * H) : 0;
        const color = levelFill(cycle.used, theme);
        const ahead =
          cycle.current && projected != null && projected > cycle.used + 1
            ? Math.min(100, projected)
            : null;
        const pixels = (share / 100) * width;
        const fade = cycle.current ? 0.55 : 1;
        return (
          <div key={cycle.start}>
            <div
              onPointerEnter={(event) => {
                setHovered(cycle);
                setPointer(event.clientX);
              }}
              style={{
                position: "absolute",
                bottom: 0,
                left: `${left}%`,
                width: `${share}%`,
                height: Math.max(height, 4),
                opacity: fade,
                borderRadius: long ? 0 : "2px 2px 0 0",
                background: !height
                  ? "transparent"
                  : long
                    ? `linear-gradient(${color}, ${color}) top / 100% 2px no-repeat, color-mix(in srgb, ${color} 14%, transparent)`
                    : `color-mix(in srgb, ${color} ${cycle.used >= 70 ? 90 : 55}%, transparent)`,
              }}
            />
            {ahead != null && (
              <div
                style={{
                  position: "absolute",
                  left: `${x(now)}%`,
                  width: `${Math.max(0, x(cycle.end) - x(now))}%`,
                  bottom: (ahead / 100) * H,
                  borderTop: `1.5px dotted ${levelFill(ahead, theme)}`,
                  pointerEvents: "none",
                }}
              />
            )}
            {pixels >= 24 && cycle.used > 0 && (
              <div
                style={{
                  ...number,
                  position: "absolute",
                  left: `${left + share / 2}%`,
                  bottom:
                    Math.max(height, ahead != null ? (ahead / 100) * H : 0) + 3,
                  transform: "translateX(-50%)",
                  fontSize: 10,
                  color: cycle.used >= 70 ? color : theme.tertiary,
                  whiteSpace: "nowrap",
                  pointerEvents: "none",
                }}
              >
                {cycle.partial ? "≥" : ""}
                {percent(cycle.used)}
                {ahead != null ? ` → ${percent(ahead)}` : ""}
              </div>
            )}
          </div>
        );
      })}
      {hovered && (
        <div
          role="tooltip"
          style={{
            ...card(theme),
            position: "absolute",
            zIndex: 10,
            pointerEvents: "none",
            bottom: H + 16,
            left: Math.max(
              0,
              Math.min(
                width - 200,
                pointer -
                  (ref.current?.getBoundingClientRect().left ?? 0) -
                  100,
              ),
            ),
            width: 200,
            padding: "8px 10px",
            background: theme.ground,
            boxShadow: "0 5px 18px #0002",
            fontSize: 11,
            display: "grid",
            gap: 2,
          }}
        >
          <span style={{ color: theme.secondary }}>
            {time.format(hovered.start)} – {time.format(hovered.end)}
          </span>
          <span>
            {hovered.current
              ? zh
                ? "已用 "
                : "Used so far "
              : zh
                ? "用掉 "
                : "Used "}
            <strong style={number}>
              {hovered.partial ? "≥" : ""}
              {percent(hovered.used)}
            </strong>
          </span>
          {hovered.current && projected != null && projected > hovered.used && (
            <span style={{ color: theme.secondary }}>
              {zh ? "按现在的速度到重置约 " : "About "}
              <strong style={number}>
                {percent(Math.min(100, projected))}
              </strong>
              {zh ? "" : " by its reset at this pace"}
            </span>
          )}
          {hovered.partial && (
            <span style={{ color: theme.tertiary }}>
              {zh
                ? "离重置还早就没有再读到，可能用了更多"
                : "Not read again before its reset; it may have used more"}
            </span>
          )}
          <span style={{ color: theme.tertiary }}>
            {zh ? `${hovered.readings} 次读数` : `${hovered.readings} readings`}
          </span>
        </div>
      )}
    </div>
  );
}
