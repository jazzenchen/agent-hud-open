// The lanes of a picked turn's sub-agent timeline: who ran when, each step
// between calls, what each row added, and the line its hover card reads.
// The timeline only draws them.
import type { SessionCall, SessionTurnAgent } from "@agent-hud/protocol";
import {
  tokensShort as compact,
  translate,
  type Language,
} from "@agent-hud/ui/format";
import type { Theme } from "./SessionTheme";

/** A step (one call to the next) at least this long: long thinking or a slow tool. */
const LONG_STEP = 15_000;
const LIGHT_PALETTE =
  "#398ad6,#00a091,#9470cd,#c26e12,#c35f92,#5f8f2a,#a4801f".split(",");
const DARK_PALETTE =
  "#6db0f4,#31c3b5,#b699eb,#e29858,#e38ab5,#a5c862,#d9b75a".split(",");

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

type TimedCall = { at: number; call: SessionCall };
type OrderedAgent = [
  agent: SessionTurnAgent,
  depth: number,
  parent: string | null,
];

const ms = (at: string | null | undefined) => {
  const value = at ? Date.parse(at) : NaN;
  return Number.isFinite(value) ? value : null;
};

function pushMapItem<K, V>(map: Map<K, V[]>, key: K, value: V) {
  const list = map.get(key);
  if (list) list.push(value);
  else map.set(key, [value]);
}

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

function mostUsedModel(entries: readonly { call: SessionCall }[]) {
  const counts = new Map<string, number>();
  for (const { call } of entries) {
    const name = call.modelName || call.model;
    if (name) counts.set(name, (counts.get(name) ?? 0) + 1);
  }
  let best: string | null = null;
  let bestCount = 0;
  for (const [name, count] of counts) {
    if (count > bestCount) {
      best = name;
      bestCount = count;
    }
  }
  return best;
}

function mergeSubagentIntervals(rows: readonly Lane[]): [number, number][] {
  if (rows.length <= 1) return [];
  const raw: [number, number][] = [];
  for (let i = 1; i < rows.length; i += 1)
    raw.push([rows[i].start, rows[i].end]);
  raw.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const merged: [number, number][] = [raw[0]];
  for (let i = 1; i < raw.length; i += 1) {
    const [start, end] = raw[i];
    const last = merged[merged.length - 1];
    if (start <= last[1]) {
      if (end > last[1]) last[1] = end;
    } else merged.push([start, end]);
  }
  return merged;
}

function overlapsMerged(
  merged: readonly [number, number][],
  a: number,
  b: number,
) {
  let lo = 0;
  let hi = merged.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if (merged[mid][1] <= a) lo = mid + 1;
    else hi = mid;
  }
  return lo < merged.length && merged[lo][0] < b;
}

function sum(entries: readonly TimedCall[]) {
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
}

function buildSteps(
  start: number,
  end: number,
  entries: readonly { at: number }[],
  running: boolean,
): Step[] {
  const out: Step[] = [];
  let previous = start;
  for (const { at: point } of entries) {
    if (point > previous) {
      out.push({
        from: previous,
        to: point,
        kind: point - previous >= LONG_STEP ? "long" : "work",
      });
      previous = point;
    }
  }
  if (end > previous)
    out.push({
      from: previous,
      to: end,
      kind: running ? "live" : end - previous >= LONG_STEP ? "long" : "work",
    });
  return out;
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
  language: Language,
): Lane[] {
  const palette = theme.light ? LIGHT_PALETTE : DARK_PALETTE;
  const timed: TimedCall[] = [];
  for (const call of calls) {
    const at = ms(call.timestamp);
    if (at !== null) timed.push({ at, call });
  }
  timed.sort((a, b) => a.at - b.at);

  const ownCalls: TimedCall[] = [];
  let ownLog: string | null = null;
  const callsByLog = new Map<string, TimedCall[]>();
  for (const entry of timed) {
    if (!entry.call.subagent) {
      ownCalls.push(entry);
      if (ownLog === null && entry.call.log) ownLog = entry.call.log;
    }
    if (entry.call.log !== undefined)
      pushMapItem(callsByLog, entry.call.log, entry);
  }

  const known = new Map<string, SessionTurnAgent>();
  for (const agent of agents) known.set(agent.log, agent);

  // Depth-first under each parent, siblings by when they started.
  const children = new Map<string | null, SessionTurnAgent[]>();
  const startByLog = new Map<string, number>();
  for (const agent of known.values()) {
    const parent =
      agent.parent && agent.parent !== ownLog && known.has(agent.parent)
        ? agent.parent
        : null;
    pushMapItem(children, parent, agent);
    const firstAt = callsByLog.get(agent.log)?.[0]?.at ?? until;
    startByLog.set(agent.log, Math.max(from, ms(agent.startedAt) ?? firstAt));
  }

  const ordered: OrderedAgent[] = [];
  const stack: OrderedAgent[] = [];
  const pushChildren = (parent: string | null, depth: number) => {
    const group = children.get(parent);
    if (!group) return;
    group.sort((a, b) => startByLog.get(a.log)! - startByLog.get(b.log)!);
    for (let i = group.length - 1; i >= 0; i -= 1)
      stack.push([group[i], depth, parent]);
  };
  pushChildren(null, 1);
  while (stack.length > 0) {
    const item = stack.pop()!;
    ordered.push(item);
    pushChildren(item[0].log, item[1] + 1);
  }

  const main: Lane = {
    key: ownLog ?? "own",
    name: translate(language, "Main session"),
    sub: mostUsedModel(ownCalls) ?? "",
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
  const emptyCalls: TimedCall[] = [];
  for (let position = 0; position < ordered.length; position += 1) {
    const [agent, depth, parent] = ordered[position];
    const own = callsByLog.get(agent.log) ?? emptyCalls;
    const start = startByLog.get(agent.log)!;
    const lastCall = own.at(-1)?.at ?? start;
    const heard = ms(agent.lastAt);
    const rawEnd = agent.running
      ? now
      : Math.max(lastCall, heard !== null ? Math.min(heard, until) : lastCall);
    const end = Math.max(start, rawEnd);
    index.set(agent.log, result.length);
    result.push({
      key: agent.log,
      name: agent.name,
      sub: [agent.kind, mostUsedModel(own)].filter(Boolean).join(" · "),
      depth,
      color: palette[position % palette.length],
      parent: index.get(parent) ?? 0,
      start,
      end,
      running: agent.running,
      calls: own === emptyCalls ? [] : own,
      steps: buildSteps(start, end, own, agent.running),
      ...sum(own),
    });
  }
  // The session's row runs until its last sub-agent hands back, so the wait shows.
  if (!live) {
    for (let i = 1; i < result.length; i += 1)
      if (result[i].end > main.end) main.end = result[i].end;
  }
  // The session waits while its sub-agents work: a long gap of its own then is idle.
  const mergedBusy = mergeSubagentIntervals(result);
  main.steps = buildSteps(main.start, main.end, ownCalls, live).map((step) =>
    step.kind === "long" && overlapsMerged(mergedBusy, step.from, step.to)
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
  language: Language,
) {
  if (nearest) {
    const tools = nearest.call.tools?.length
      ? nearest.call.tools.join(", ")
      : translate(language, "reply");
    const added = translate(language, "{{tokens}} new", {
      tokens: compact(nearest.call.newTokens ?? 0),
    });
    return `+${offset(nearest.at - start)} · ${tools} · ${added}`;
  }
  if (step) {
    const length = offset(step.to - step.from);
    const text =
      step.kind === "idle"
        ? translate(language, "Waiting on sub-agents {{length}}", { length })
        : step.kind === "live"
          ? translate(language, "Working {{length}}", { length })
          : translate(language, "A step of {{length}}", { length });
    return `+${offset(at - start)} · ${text}`;
  }
  const calls = translate(language, "{{count}} calls", {
    count: lane.calls.length,
  });
  return `+${offset(at - start)} · ${calls}`;
}
