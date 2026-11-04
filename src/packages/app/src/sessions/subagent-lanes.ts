// The lanes of a picked turn's sub-agent timeline: who ran when, each step
// between calls, what each row added, and the line its hover card reads.
// The timeline only draws them.
import type { SessionCall, SessionTurnAgent } from "@agent-hud/protocol";
import { compact, type Theme } from "./SessionTheme";

/** A step (one call to the next) at least this long: long thinking or a slow tool. */
const LONG_STEP = 15_000;
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

export interface Step {
  from: number;
  to: number;
  kind: "work" | "long" | "idle" | "live";
}

export interface Lane {
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

const ms = (at: string | null | undefined) => {
  const value = at ? Date.parse(at) : NaN;
  return Number.isFinite(value) ? value : null;
};

/** "0s", "45s", "2m", "2m30s", "1h05m". */
export function offset(milliseconds: number) {
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
export function lanes(
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
  const known = new Map(agents.map((agent) => [agent.log, agent]));
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

export function tipLine(
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
