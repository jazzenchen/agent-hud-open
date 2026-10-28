import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { MouseEvent as ReactMouseEvent, ReactNode } from "react";
import type { SessionCall, SessionTurnAgent } from "../lib/types";
import {
  clipped,
  compact,
  cost,
  numeric,
  plain,
  row,
  type Theme,
} from "./SessionTheme";

/** A step (one call to the next) at least this long: long thinking or a slow tool. */
const LONG_STEP = 15_000;
const ROW = 36;
const BAR = 12;
const PALETTE = {
  light: [
    "#398ad6",
    "#00a091",
    "#9470cd",
    "#c26e12",
    "#c35f92",
    "#5f8f2a",
    "#a4801f",
  ],
  dark: [
    "#6db0f4",
    "#31c3b5",
    "#b699eb",
    "#e29858",
    "#e38ab5",
    "#a5c862",
    "#d9b75a",
  ],
};
const AXIS_STEPS = [
  1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200,
];

interface Step {
  from: number;
  to: number;
  kind: "work" | "long" | "idle" | "live";
}

interface Lane {
  key: string;
  name: string;
  sub: string;
  depth: number;
  color: string;
  parent: number | null;
  start: number;
  end: number;
  running: boolean;
  calls: { at: number; call: SessionCall }[];
  steps: Step[];
  added: number;
  cost: number | null;
  unpriced: boolean;
}

interface Hover {
  lane: number;
  at: number;
  x: number;
  y: number;
}

const ms = (at: string | null | undefined) => {
  const value = at ? Date.parse(at) : NaN;
  return Number.isFinite(value) ? value : null;
};

/** "0s", "45s", "2m", "2m30s", "1h05m". */
function offset(milliseconds: number) {
  const seconds = Math.max(0, Math.round(milliseconds / 1000));
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    const rest = seconds % 60;
    return rest
      ? `${minutes}m${String(rest).padStart(2, "0")}s`
      : `${minutes}m`;
  }
  return `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, "0")}m`;
}

function mostUsedModel(calls: SessionCall[]) {
  const counts = new Map<string, number>();
  for (const call of calls) {
    const name = call.modelName || call.model;
    if (name) counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  return [...counts].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null;
}

/** The turn's rows: the session itself, then each sub-agent under the log that started it. */
function lanes(
  calls: SessionCall[],
  agents: SessionTurnAgent[],
  from: number,
  until: number,
  now: number,
  live: boolean,
  theme: Theme,
): Lane[] {
  const zh = Boolean(theme.zh);
  const palette = theme.light ? PALETTE.light : PALETTE.dark;
  const timed = calls
    .map((call) => ({ at: ms(call.timestamp), call }))
    .filter(
      (entry): entry is { at: number; call: SessionCall } => entry.at !== null,
    )
    .sort((a, b) => a.at - b.at);
  const ownCalls = timed.filter((entry) => !entry.call.subagent);
  const ownLog = ownCalls.find((entry) => entry.call.log)?.call.log ?? null;
  // Sub-agent logs the engine did not describe (an older engine) still get a row.
  const known = new Map(agents.map((agent) => [agent.log, agent]));
  let unnamed = 0;
  for (const { call } of timed)
    if (call.subagent && call.log && !known.has(call.log))
      known.set(call.log, {
        log: call.log,
        name: zh ? `子 agent ${++unnamed}` : `Sub-agent ${++unnamed}`,
        kind: null,
        parent: ownLog,
        startedAt: null,
        lastAt: null,
        running: false,
      });
  // Depth-first under each parent, siblings by when they started.
  const children = new Map<string | null, SessionTurnAgent[]>();
  for (const agent of known.values()) {
    const parent =
      agent.parent && agent.parent !== ownLog && known.has(agent.parent)
        ? agent.parent
        : null;
    children.set(parent, [...(children.get(parent) ?? []), agent]);
  }
  const firstCall = (log: string) =>
    timed.find((entry) => entry.call.log === log)?.at ?? until;
  const started = (agent: SessionTurnAgent) =>
    Math.max(from, ms(agent.startedAt) ?? firstCall(agent.log));
  const ordered: {
    agent: SessionTurnAgent;
    depth: number;
    parent: string | null;
  }[] = [];
  const visit = (parent: string | null, depth: number) => {
    for (const agent of (children.get(parent) ?? []).sort(
      (a, b) => started(a) - started(b),
    )) {
      ordered.push({ agent, depth, parent });
      visit(agent.log, depth + 1);
    }
  };
  visit(null, 1);

  const sum = (entries: { call: SessionCall }[]) => {
    let added = 0;
    let money = 0;
    let unpriced = false;
    let priced = false;
    for (const { call } of entries) {
      added += call.newTokens ?? 0;
      if (call.listCost == null) unpriced = true;
      else {
        money += call.listCost;
        priced = true;
      }
    }
    return { added, cost: priced ? money : null, unpriced };
  };
  const steps = (
    start: number,
    end: number,
    at: number[],
    running: boolean,
  ) => {
    const out: Step[] = [];
    let previous = start;
    for (const point of at) {
      if (point > previous)
        out.push({
          from: previous,
          to: point,
          kind: point - previous >= LONG_STEP ? "long" : "work",
        });
      previous = Math.max(previous, point);
    }
    if (end > previous)
      out.push({
        from: previous,
        to: end,
        kind: running ? "live" : end - previous >= LONG_STEP ? "long" : "work",
      });
    return out;
  };

  const main: Lane = {
    key: ownLog ?? "own",
    name: zh ? "主会话" : "Main session",
    sub: mostUsedModel(ownCalls.map((entry) => entry.call)) ?? "",
    depth: 0,
    color: theme.secondary,
    parent: null,
    start: from,
    end: live ? now : Math.max(from, ownCalls.at(-1)?.at ?? from),
    running: live,
    calls: ownCalls,
    steps: [],
    ...sum(ownCalls),
  };
  const result: Lane[] = [main];
  const index = new Map<string | null, number>([[null, 0]]);
  for (const [position, { agent, depth, parent }] of ordered.entries()) {
    const own = timed.filter((entry) => entry.call.log === agent.log);
    const start = started(agent);
    const lastCall = own.at(-1)?.at ?? start;
    const heard = ms(agent.lastAt);
    const end = agent.running
      ? now
      : Math.max(lastCall, heard !== null ? Math.min(heard, until) : lastCall);
    index.set(agent.log, result.length);
    const model = mostUsedModel(own.map((entry) => entry.call));
    result.push({
      key: agent.log,
      name: agent.name,
      sub: [agent.kind, model].filter(Boolean).join(" · "),
      depth,
      color: palette[position % palette.length],
      parent: index.get(parent) ?? 0,
      start,
      end: Math.max(start, end),
      running: agent.running,
      calls: own,
      steps: steps(
        start,
        Math.max(start, end),
        own.map((entry) => entry.at),
        agent.running,
      ),
      ...sum(own),
    });
  }
  // The session's row runs until its last sub-agent hands back, so the wait shows.
  if (!live)
    main.end = Math.max(main.end, ...result.slice(1).map((lane) => lane.end));
  // The session waits while its sub-agents work: a long gap of its own then is idle.
  const busy = (a: number, b: number) =>
    result.some((lane, i) => i > 0 && lane.start < b && lane.end > a);
  main.steps = steps(
    main.start,
    main.end,
    ownCalls.map((entry) => entry.at),
    live,
  ).map((step) =>
    step.kind === "long" && busy(step.from, step.to)
      ? { ...step, kind: "idle" as const }
      : step,
  );
  return result;
}

/** A lane's calls in view, one tick each, but none within 0.4% of the view of
 * the last one kept: hundreds of calls would otherwise merge into a block. */
function ticks(lane: Lane, from: number, to: number, span: number) {
  const kept: { at: number; call: SessionCall }[] = [];
  for (const entry of lane.calls) {
    if (entry.at < from || entry.at > to) continue;
    const last = kept.at(-1);
    if (!last || entry.at - last.at >= span * 0.004) kept.push(entry);
  }
  return kept;
}

/**
 * A picked turn's work over time: the session and each sub-agent it sent, one
 * row each, from when it started to its last call. A tick is a model call, a
 * darker stretch a step of 15 s or more; dots on the parent row mark sending
 * and handing back. Drag across the plot to zoom; double-click to return.
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
  const [view, setView] = useState<[number, number] | null>(null);
  const [from, to] = view ?? [start, fullTo];
  const span = Math.max(1, to - from);
  const pos = (at: number) => ((at - from) / span) * 100;
  const width = (a: number, b: number) =>
    Math.max(0, pos(Math.min(b, to)) - pos(Math.max(a, from)));
  const visible = (a: number, b: number) => b >= from && a <= to;

  const plot = useRef<HTMLDivElement>(null);
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
  const nearest =
    hover && hovered
      ? hovered.calls.reduce<{ at: number; call: SessionCall } | null>(
          (best, entry) =>
            Math.abs(entry.at - hover.at) < span * 0.015 &&
            (!best ||
              Math.abs(entry.at - hover.at) < Math.abs(best.at - hover.at))
              ? entry
              : best,
          null,
        )
      : null;
  const hoveredStep =
    hover && hovered && !nearest
      ? hovered.steps.find(
          (step) => step.from <= hover.at && step.to >= hover.at,
        )
      : undefined;

  const stepColor = (lane: Lane, kind: Step["kind"]) =>
    `color-mix(in srgb, ${lane.color} ${kind === "long" ? 55 : kind === "live" ? 40 : 22}%, transparent)`;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <div style={{ display: "flex", gap: 12, alignItems: "flex-start" }}>
        {/* Names */}
        <div style={{ width: 200, flexShrink: 0 }}>
          <div style={{ height: 16, fontSize: 11, color: theme.secondary }}>
            agent
          </div>
          {rows.map((lane, i) => (
            <button
              key={lane.key}
              type="button"
              onClick={() => setFocus(focus === i ? null : i)}
              aria-pressed={focus === i}
              aria-label={`${lane.name}${lane.sub ? `, ${lane.sub}` : ""}`}
              style={{
                ...plain,
                ...row,
                gap: 7,
                width: "100%",
                height: ROW,
                paddingLeft: Math.min(lane.depth, 3) * 10,
                boxSizing: "border-box",
                borderRadius: 6,
                background: focus === i ? theme.pick : "transparent",
                opacity: dim(i),
                transition: "opacity 150ms ease",
              }}
            >
              <span
                aria-hidden="true"
                style={{
                  width: 3,
                  height: 22,
                  borderRadius: 2,
                  background: lane.color,
                  flexShrink: 0,
                }}
              />
              <span
                style={{
                  display: "flex",
                  flexDirection: "column",
                  minWidth: 0,
                }}
              >
                <span
                  style={{
                    ...clipped,
                    fontSize: 12,
                    fontWeight: lane.depth === 0 ? 600 : 500,
                  }}
                >
                  {lane.name}
                </span>
                {lane.sub && (
                  <span
                    style={{
                      ...clipped,
                      fontSize: 10.5,
                      color: theme.secondary,
                    }}
                  >
                    {lane.sub}
                  </span>
                )}
              </span>
            </button>
          ))}
        </div>

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
              setHover({ lane, at: point.at, x: point.x, y: point.y });
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
                  setView([a, Math.max(b, a + 1000)]);
              }
              setDrag(null);
            }}
            onMouseLeave={() => {
              setHover(null);
              setDrag(null);
            }}
            onDoubleClick={() => setView(null)}
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
                  {ticks(lane, from, to, span).map((entry, n) => (
                    <div
                      key={`t${n}`}
                      style={{
                        position: "absolute",
                        left: `calc(${pos(entry.at)}% - 1px)`,
                        top: top - 2,
                        width: 2,
                        height: BAR + 4,
                        borderRadius: 1,
                        background: lane.color,
                      }}
                    />
                  ))}
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
                  left: Math.min(
                    hover.x + 12,
                    (plot.current?.clientWidth ?? 400) - 224,
                  ),
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
                  {tipLine(hovered, hover.at, start, nearest, hoveredStep, zh)}
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

      <div
        style={{
          ...row,
          flexWrap: "wrap",
          columnGap: 14,
          rowGap: 4,
          fontSize: 10.5,
          color: theme.secondary,
        }}
      >
        <Key>
          <span
            style={{
              width: 2,
              height: 10,
              borderRadius: 1,
              background: theme.secondary,
            }}
          />
          {zh ? "模型调用" : "Model call"}
        </Key>
        <Key>
          <span
            style={{
              width: 14,
              height: 8,
              background: `color-mix(in srgb, ${theme.secondary} 55%, transparent)`,
            }}
          />
          {zh ? "长步骤（一步 ≥15 秒）" : "Long step (≥15 s)"}
        </Key>
        <Key>
          <span
            style={{ width: 14, borderTop: `1.5px dotted ${theme.tertiary}` }}
          />
          {zh ? "主会话等待" : "Session waiting"}
        </Key>
        <Key>
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: 3,
              background: theme.secondary,
            }}
          />
          {zh ? "派出 / 交回" : "Sent / handed back"}
        </Key>
        {view ? (
          <button
            type="button"
            onClick={() => setView(null)}
            style={{ ...plain, color: theme.action, marginLeft: "auto" }}
          >
            {zh
              ? `已放大 ${offset(to - from)} · 还原整轮`
              : `Zoomed to ${offset(to - from)} · Show the whole turn`}
          </button>
        ) : (
          <span style={{ marginLeft: "auto" }}>
            {zh ? "拖选一段放大" : "Drag across to zoom"}
          </span>
        )}
      </div>
    </div>
  );
});

function Key({ children }: { children: ReactNode }) {
  return <span style={{ ...row, gap: 5 }}>{children}</span>;
}

function clock(at: number, zh: boolean) {
  return new Intl.DateTimeFormat(zh ? "zh-CN" : "en-GB", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(new Date(at));
}

function tipLine(
  lane: Lane,
  at: number,
  start: number,
  nearest: { at: number; call: SessionCall } | null,
  step: Step | undefined,
  zh: boolean,
) {
  if (nearest) {
    const tools = nearest.call.tools?.length
      ? nearest.call.tools.join(", ")
      : zh
        ? "回复"
        : "reply";
    return `+${offset(nearest.at - start)} · ${tools} · ${compact(nearest.call.newTokens ?? 0)} ${zh ? "新增" : "new"}`;
  }
  if (step) {
    const length = offset(step.to - step.from);
    const text =
      step.kind === "idle"
        ? zh
          ? `等子 agent ${length}`
          : `Waiting on sub-agents ${length}`
        : step.kind === "live"
          ? zh
            ? `进行中 ${length}`
            : `Working ${length}`
          : zh
            ? `一步 ${length}`
            : `A step of ${length}`;
    return `+${offset(at - start)} · ${text}`;
  }
  const calls = lane.calls.length;
  return `+${offset(at - start)} · ${zh ? `${calls} 次调用` : `${calls} call${calls === 1 ? "" : "s"}`}`;
}
