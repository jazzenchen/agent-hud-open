import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent } from "react";
import type { SessionCall, SessionTurnAgent } from "@agent-hud/protocol";
import { callRuns } from "../lib/session-charts";
import { lanes, offset, tipLine, type Lane, type Step } from "./subagent-lanes";
import { SubagentTimelineLegend } from "./SubagentTimelineLegend";
import { SubagentTimelineNames } from "./SubagentTimelineNames";
import { clipped, compact, cost, numeric, type Theme } from "./SessionTheme";
import { useTimelineGlide } from "./use-timeline-glide";

const ROW = 36;
const BAR = 12;
const AXIS_STEPS = [
  1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200,
];
/** Calls closer than this on screen, px, draw as one run. */
const RUN_GAP = 12;

interface Hover {
  lane: number;
  x: number;
  y: number;
}

/**
 * A picked turn's work over time: the session and each sub-agent it sent, one
 * row each, from when it started to its last call. A tick is a model call and
 * a bar a run of calls too close to tell apart, a darker stretch a step of
 * 15 s or more; dots on the parent row mark sending and handing back. Drag
 * across the plot to zoom, swipe sideways (or shift-scroll) to pan the zoomed
 * view, double-click to return.
 */
export const SubagentTimeline = memo(function SubagentTimeline({
  calls,
  agents,
  start,
  end,
  live,
  theme,
}: {
  calls: SessionCall[];
  agents: SessionTurnAgent[];
  /** The turn's prompt, ms. */
  start: number;
  /** The turn's last call, ms. */
  end: number;
  /** The session is still running this turn. */
  live: boolean;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const [now, setNow] = useState(() => Date.now());
  const anyRunning = live || agents.some((agent) => agent.running);
  useEffect(() => {
    if (!anyRunning) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [anyRunning]);
  const rows = useMemo(
    () => lanes(calls, agents, start, Math.max(end, start), now, live, theme),
    [calls, agents, start, end, now, live, theme],
  );
  const fullTo = Math.max(start + 1000, end, ...rows.map((lane) => lane.end));
  const plot = useRef<HTMLDivElement>(null);
  const { view, aim, plotWidth, glideTo } = useTimelineGlide(
    plot,
    start,
    fullTo,
  );
  const [from, to] = view ?? [start, fullTo];
  const span = Math.max(1, to - from);
  const pos = (at: number) => ((at - from) / span) * 100;
  const width = (a: number, b: number) =>
    Math.max(0, pos(Math.min(b, to)) - pos(Math.max(a, from)));
  const visible = (a: number, b: number) => b >= from && a <= to;
  const runGap = (RUN_GAP / Math.max(1, plotWidth)) * span;

  const [hover, setHover] = useState<Hover | null>(null);
  const [focus, setFocus] = useState<number | null>(null);
  const [drag, setDrag] = useState<{ a: number; b: number } | null>(null);
  const atX = (event: ReactMouseEvent) => {
    const box = plot.current?.getBoundingClientRect();
    if (!box) return null;
    const x = Math.min(Math.max(event.clientX - box.left, 0), box.width);
    return {
      x,
      y: event.clientY - box.top,
      at: from + (x / box.width) * span,
      box,
    };
  };
  const plotHeight = rows.length * ROW;
  const dim = (lane: number) =>
    focus === null ||
    focus === lane ||
    rows[lane].parent === focus ||
    rows[focus].parent === lane
      ? 1
      : 0.3;

  // About five labels along the axis, on round offsets from the prompt.
  const stepSeconds =
    AXIS_STEPS.find((seconds) => span / 1000 / seconds <= 6) ?? 7200;
  const axis: number[] = [];
  for (
    let t =
      Math.ceil((from - start) / (stepSeconds * 1000)) * stepSeconds * 1000;
    start + t <= to;
    t += stepSeconds * 1000
  )
    axis.push(start + t);

  const hovered = hover ? rows[hover.lane] : null;
  // From the pointer's x, so the card follows what slides under it while panning.
  const hoverAt = hover ? from + (hover.x / Math.max(1, plotWidth)) * span : 0;
  const nearest =
    hover && hovered
      ? hovered.calls.reduce<{ at: number; call: SessionCall } | null>(
          (best, entry) =>
            Math.abs(entry.at - hoverAt) < span * 0.015 &&
            (!best ||
              Math.abs(entry.at - hoverAt) < Math.abs(best.at - hoverAt))
              ? entry
              : best,
          null,
        )
      : null;
  const hoveredStep =
    hover && hovered && !nearest
      ? hovered.steps.find((step) => step.from <= hoverAt && step.to >= hoverAt)
      : undefined;

  const stepColor = (lane: Lane, kind: Step["kind"]) =>
    `color-mix(in srgb, ${lane.color} ${kind === "long" ? 55 : kind === "live" ? 40 : 22}%, transparent)`;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
        <SubagentTimelineNames
          rows={rows}
          rowHeight={ROW}
          focus={focus}
          onFocus={setFocus}
          dim={dim}
          theme={theme}
        />

        {/* Plot */}
        <div style={{ flex: "1 1 auto", minWidth: 120 }}>
          <div
            style={{
              ...numeric,
              height: 16,
              display: "flex",
              justifyContent: "space-between",
              fontSize: 11,
              color: theme.secondary,
            }}
          >
            <span>{clock(from, zh)}</span>
            <span>{clock(to, zh)}</span>
          </div>
          <div
            ref={plot}
            role="img"
            aria-label={
              zh
                ? `主会话与 ${rows.length - 1} 个子 agent 的时间轴`
                : `Timeline of the session and ${rows.length - 1} sub-agents`
            }
            onMouseMove={(event) => {
              const point = atX(event);
              if (!point) return;
              if (drag) setDrag({ ...drag, b: point.at });
              const lane = Math.min(
                rows.length - 1,
                Math.max(0, Math.floor(point.y / ROW)),
              );
              setHover({ lane, x: point.x, y: point.y });
            }}
            onMouseDown={(event) => {
              const point = atX(event);
              if (point && event.button === 0)
                setDrag({ a: point.at, b: point.at });
            }}
            onMouseUp={(event) => {
              const point = atX(event);
              if (drag && point) {
                const [a, b] = [drag.a, point.at].sort((x, y) => x - y);
                if (((b - a) / span) * point.box.width > 6)
                  glideTo([a, Math.max(b, a + 1000)]);
              }
              setDrag(null);
            }}
            onMouseLeave={() => {
              setHover(null);
              setDrag(null);
            }}
            onDoubleClick={() => glideTo(null)}
            style={{
              position: "relative",
              height: plotHeight,
              overflow: "hidden",
              cursor: "crosshair",
              userSelect: "none",
            }}
          >
            {axis.map((at) => (
              <div
                key={`g${at}`}
                style={{
                  position: "absolute",
                  left: `${pos(at)}%`,
                  top: 0,
                  width: 1,
                  height: plotHeight,
                  background: theme.divider,
                }}
              />
            ))}
            {/* Sending: a line from the parent row down to the row it started. */}
            {rows.map((lane, i) =>
              i === 0 ||
              lane.parent === null ||
              !visible(lane.start, lane.start) ? null : (
                <div
                  key={`c${lane.key}`}
                  style={{
                    position: "absolute",
                    left: `${pos(lane.start)}%`,
                    top: lane.parent * ROW + ROW / 2,
                    width: 1,
                    height: (i - lane.parent) * ROW,
                    background: lane.color,
                    opacity: 0.45 * dim(i),
                  }}
                />
              ),
            )}
            {rows.map((lane, i) => {
              const top = i * ROW + (ROW - BAR) / 2;
              return (
                <div key={lane.key} style={{ opacity: dim(i) }}>
                  {lane.steps
                    .filter((step) => visible(step.from, step.to))
                    .map((step) =>
                      step.kind === "idle" ? (
                        <div
                          key={`s${step.from}`}
                          style={{
                            position: "absolute",
                            left: `${pos(Math.max(step.from, from))}%`,
                            width: `${width(step.from, step.to)}%`,
                            top: top + BAR / 2,
                            borderTop: `1.5px dotted ${theme.tertiary}`,
                          }}
                        />
                      ) : (
                        <div
                          key={`s${step.from}`}
                          style={{
                            position: "absolute",
                            left: `${pos(Math.max(step.from, from))}%`,
                            width: `${width(step.from, step.to)}%`,
                            top,
                            height: BAR,
                            background: stepColor(lane, step.kind),
                          }}
                        />
                      ),
                    )}
                  {callRuns(lane.calls, from, to, runGap).map((run) => {
                    // A lone call, or calls too close to spread, stand taller
                    // than the bar as a tick; a run lies flush with it.
                    const tick = run.to - run.from < runGap / 3;
                    return (
                      <div
                        key={`t${run.from}`}
                        style={{
                          position: "absolute",
                          left: `calc(${pos(run.from)}% - 1px)`,
                          width: `calc(${pos(run.to) - pos(run.from)}% + 2px)`,
                          top: tick ? top - 2 : top,
                          height: tick ? BAR + 4 : BAR,
                          borderRadius: tick ? 1 : 2,
                          background: lane.color,
                        }}
                      />
                    );
                  })}
                  {lane.running && visible(lane.end, lane.end) && (
                    <div
                      style={{
                        position: "absolute",
                        left: `calc(${pos(lane.end)}% - 1px)`,
                        top: top - 3,
                        width: 2,
                        height: BAR + 6,
                        borderRadius: 1,
                        background: lane.color,
                        boxShadow: `0 0 6px ${lane.color}`,
                        animation: "session-pulse 1.4s ease-in-out infinite",
                      }}
                    />
                  )}
                </div>
              );
            })}
            {/* Sent and handed back, on the parent's row. */}
            {rows.flatMap((lane, i) =>
              i === 0 || lane.parent === null
                ? []
                : [
                    { at: lane.start, key: `d${lane.key}` },
                    ...(lane.running
                      ? []
                      : [{ at: lane.end, key: `e${lane.key}` }]),
                  ]
                    .filter(({ at }) => visible(at, at))
                    .map(({ at, key }) => (
                      <div
                        key={key}
                        style={{
                          position: "absolute",
                          left: `calc(${pos(at)}% - 3px)`,
                          top: (lane.parent ?? 0) * ROW + ROW / 2 - 3,
                          width: 6,
                          height: 6,
                          borderRadius: 3,
                          background: lane.color,
                          boxShadow: `0 0 0 1.5px ${theme.surface}`,
                          opacity: dim(i),
                        }}
                      />
                    )),
            )}
            {anyRunning && visible(now, now) && (
              <div
                style={{
                  position: "absolute",
                  left: `${pos(now)}%`,
                  top: 0,
                  width: 1.5,
                  height: plotHeight,
                  background: theme.accent,
                }}
              />
            )}
            {drag && Math.abs(drag.b - drag.a) > 0 && (
              <div
                style={{
                  position: "absolute",
                  left: `${pos(Math.min(drag.a, drag.b))}%`,
                  width: `${Math.abs(pos(drag.b) - pos(drag.a))}%`,
                  top: 0,
                  height: plotHeight,
                  background: `color-mix(in srgb, ${theme.action} 14%, transparent)`,
                  borderLeft: `1px solid ${theme.action}`,
                  borderRight: `1px solid ${theme.action}`,
                }}
              />
            )}
            {hover && hovered && !drag && (
              <div
                style={{
                  position: "absolute",
                  left: Math.min(hover.x + 12, plotWidth - 224),
                  top: Math.min(hover.y + 12, Math.max(0, plotHeight - 64)),
                  width: 212,
                  boxSizing: "border-box",
                  padding: "6px 8px",
                  borderRadius: 6,
                  background: theme.surface,
                  border: `1px solid ${theme.border}`,
                  boxShadow: "0 4px 14px rgba(0,0,0,.22)",
                  fontSize: 11,
                  lineHeight: "15px",
                  pointerEvents: "none",
                  zIndex: 1,
                }}
              >
                <div style={{ ...clipped, fontWeight: 600 }}>
                  {hovered.name}
                </div>
                <div style={{ color: theme.secondary }}>
                  {tipLine(hovered, hoverAt, start, nearest, hoveredStep, zh)}
                </div>
              </div>
            )}
          </div>
          <div style={{ position: "relative", height: 18 }}>
            {axis.map((at) => (
              <span
                key={`a${at}`}
                style={{
                  ...numeric,
                  position: "absolute",
                  top: 3,
                  left: `${pos(at)}%`,
                  transform: pos(at) < 4 ? "none" : "translateX(-50%)",
                  fontSize: 10.5,
                  color: theme.secondary,
                }}
              >
                {offset(at - start)}
              </span>
            ))}
          </div>
        </div>

        {/* What each row added */}
        <div style={{ width: 76, flexShrink: 0 }}>
          <div
            style={{
              height: 16,
              fontSize: 11,
              color: theme.secondary,
              textAlign: "right",
            }}
          >
            {zh ? "新增 · 费用" : "New · cost"}
          </div>
          {rows.map((lane, i) => (
            <div
              key={lane.key}
              style={{
                ...numeric,
                height: ROW,
                display: "flex",
                flexDirection: "column",
                justifyContent: "center",
                alignItems: "flex-end",
                opacity: dim(i),
              }}
            >
              <span style={{ fontSize: 12, fontWeight: 600 }}>
                {compact(lane.added)}
              </span>
              <span style={{ fontSize: 10.5, color: theme.secondary }}>
                {cost(lane.cost, zh, lane.unpriced && lane.cost !== null)}
              </span>
            </div>
          ))}
        </div>
      </div>

      <SubagentTimelineLegend
        zoomed={aim ? aim[1] - aim[0] : null}
        onShowAll={() => glideTo(null)}
        theme={theme}
      />
    </div>
  );
});

function clock(at: number, zh: boolean) {
  return new Intl.DateTimeFormat(zh ? "zh-CN" : "en-GB", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(new Date(at));
}
