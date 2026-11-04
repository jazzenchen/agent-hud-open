// What the Sessions page's charts draw, as the iPhone's and agent-hud-native-open's
// session charts do. Core counts every figure; these only place them.

/**
 * A round scale for `peak` (the iPhone's `niceStep`): about `count` steps of
 * 1, 2, 2.5, 5 or 10 times a power of ten, the top the first step at or above
 * the peak, and every step from zero. Works for dollars as for tokens.
 */
export function niceScale(
  peak: number,
  count = 3,
): { step: number; top: number; ticks: number[] } {
  const raw = peak > 0 ? peak / count : 1;
  const power = 10 ** Math.floor(Math.log10(raw));
  const multiple =
    [1, 2, 2.5, 5, 10].find((m) => m * power >= raw * (1 - 1e-12)) ?? 10;
  const step = multiple * power;
  const steps = Math.max(1, Math.ceil(peak / step - 1e-9));
  const round = (value: number) => Number(value.toPrecision(12));
  const ticks = Array.from({ length: steps + 1 }, (_, index) =>
    round(index * step),
  );
  return { step: round(step), top: ticks[ticks.length - 1], ticks };
}

/** Where a chart of `count` turns names them: the first, a third, two thirds and the last (the iPhone's ticks). */
export function turnTicks(count: number): number[] {
  if (count <= 0) return [];
  const last = count - 1;
  return [
    ...new Set([0, Math.floor(last / 3), Math.floor((2 * last) / 3), last]),
  ].sort((a, b) => a - b);
}

/** The positions of the `n` largest positive values, largest first; of equal values the earlier wins. */
export function largestPositions(
  values: readonly number[],
  n: number,
): number[] {
  return values
    .map((value, index) => ({ value, index }))
    .filter((entry) => entry.value > 0)
    .sort((a, b) => b.value - a.value || a.index - b.index)
    .slice(0, Math.max(0, n))
    .map((entry) => entry.index);
}

/** The kinds a turn adds, stacked from the base up (Open's `stackedKinds`); cache reads are the context. */
export const newKindOrder = [
  "cacheWrite",
  "input",
  "reasoning",
  "output",
] as const;
export type NewKind = (typeof newKindOrder)[number];

/** One part of a stacked bar: a kind, the session's own or its sub-agents' (drawn faded). */
export interface BarPart {
  kind: NewKind;
  value: number;
  subagent: boolean;
}

/** A bar's parts from the base up: the session's own kinds, then the sub-agents' on top. */
export interface StackedBar {
  parts: BarPart[];
  total: number;
  compacted: boolean;
}

type KindLists = Partial<Record<NewKind, number[]>> | null;
type Kinds =
  Partial<Record<NewKind | "cacheRead" | "total", number>> | null | undefined;

function stack(
  own: (kind: NewKind) => number,
  helped: (kind: NewKind) => number,
): BarPart[] {
  const parts: BarPart[] = [];
  for (const kind of newKindOrder) {
    const value = own(kind);
    if (value > 0) parts.push({ kind, value, subagent: false });
  }
  for (const kind of newKindOrder) {
    const value = helped(kind);
    if (value > 0) parts.push({ kind, value, subagent: true });
  }
  return parts;
}

/** Core's per-kind lists of a row (`Session.turnBars`) as bars to stack. */
export function stackTurnBars(bars: {
  count: number;
  own: KindLists;
  subagents: KindLists;
  compacted: readonly number[];
}): StackedBar[] {
  const compacted = new Set(bars.compacted);
  return Array.from({ length: bars.count }, (_, index) => {
    const parts = stack(
      (kind) => bars.own?.[kind]?.[index] ?? 0,
      (kind) => bars.subagents?.[kind]?.[index] ?? 0,
    );
    return {
      parts,
      total: parts.reduce((sum, part) => sum + part.value, 0),
      compacted: compacted.has(index),
    };
  });
}

/** One turn of the session page as a bar to stack: Core's own kinds, then its sub-agents'. */
export function stackTurn(
  own: Kinds,
  subagents: Kinds,
  compacted = false,
): StackedBar {
  const parts = stack(
    (kind) => own?.[kind] ?? 0,
    (kind) => subagents?.[kind] ?? 0,
  );
  return {
    parts,
    total: parts.reduce((sum, part) => sum + part.value, 0),
    compacted,
  };
}

/**
 * The context line broken at each compaction (the iPhone's
 * `ContextWindowChart`): runs of positions that have a context, a compacted
 * turn starting a new run.
 */
export function contextRuns(
  points: readonly {
    context: number | null | undefined;
    compacted?: boolean;
  }[],
): number[][] {
  const runs: number[][] = [];
  let run: number[] = [];
  points.forEach((point, index) => {
    if (point.context == null) return;
    if (point.compacted && run.length > 0) {
      runs.push(run);
      run = [];
    }
    run.push(index);
  });
  if (run.length > 0) runs.push(run);
  return runs;
}

/**
 * A timeline row's calls as runs: a call less than `gap` ms after the one
 * before joins its run, so a busy stretch draws as one bar instead of a comb
 * of ticks. A run of one call is a tick. Only runs that reach into
 * `from`–`to` come back; they are grouped over every call, so panning never
 * regroups them.
 */
export function callRuns(
  calls: readonly { at: number }[],
  from: number,
  to: number,
  gap: number,
): { from: number; to: number; calls: number }[] {
  const runs: { from: number; to: number; calls: number }[] = [];
  for (const { at } of calls) {
    const last = runs.at(-1);
    if (last && at - last.to < gap) {
      last.to = at;
      last.calls += 1;
    } else runs.push({ from: at, to: at, calls: 1 });
  }
  return runs.filter((run) => run.to >= from && run.from <= to);
}

/** A zoomed window moved `by` ms, kept inside `start`–`end` at the same length. */
export function panWindow(
  [from, to]: readonly [number, number],
  by: number,
  start: number,
  end: number,
): [number, number] {
  const length = to - from;
  const next = Math.max(start, Math.min(from + by, end - length));
  return [next, next + length];
}
