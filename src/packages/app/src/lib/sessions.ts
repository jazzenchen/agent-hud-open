// How the Statistics window names a session's client and place, as
// agent-hud-native-open's session list (SessionCard) and session page do.

interface ClientFacts {
  source: string;
  client?: string | null;
}

interface PlaceFacts {
  model: string;
  modelName?: string | null;
  cwd: string | null;
}

/**
 * The client that drives a session (Open's `SessionSource.name`): Core's name
 * for it ("Codex Desktop", "Claude Code IDE extension"), else its agent's
 * name ("DeepSeek"), else the agent's id.
 */
export function sessionClient(
  session: ClientFacts,
  providers: readonly { id: string; name: string }[] = [],
): string {
  return (
    session.client ||
    providers.find((provider) => provider.id === session.source)?.name ||
    session.source
  );
}

/**
 * A Sessions page row's second line (the phone's `SessionCard`): the model as
 * people read it, the project (Core writes it with ~) else the folder, and the
 * client that drives it.
 */
export function sessionWhere(
  session: PlaceFacts & {
    project?: string | null;
    desktopProject?: { projectLabel?: string };
  },
  client: string,
): string[] {
  return [
    session.modelName || session.model || "—",
    session.desktopProject?.projectLabel || session.project || session.cwd,
    client,
  ].filter((part): part is string => Boolean(part));
}

/**
 * When a session's last message was written (the person's prompt or the
 * agent's text reply), else its last event. Lists order, group and date
 * sessions by it, never by their state, which can read wrong (owner,
 * 2026-10-26).
 */
export function lastMessageTime(session: {
  lastMessageAt?: string | null;
  updatedAt: string;
}): string {
  return session.lastMessageAt || session.updatedAt;
}

/** A session row's tooltip: its title, its client and its folder. */
export function sessionHelp(
  session: { title: string; cwd: string | null },
  client: string,
): string {
  return [session.title, client, session.cwd].filter(Boolean).join(" · ");
}

// How the Sessions page splits its tokens by kind, as agent-hud-native-open's
// `SessionsTodayCard` does.

/** The five token kinds in Open's display order (`TokenKind.allCases`): what a call adds, then what it reads back from the cache. */
export const tokenKindOrder = [
  "cacheWrite",
  "input",
  "reasoning",
  "output",
  "cacheRead",
] as const;
export type TokenKindName = (typeof tokenKindOrder)[number];

/** The kinds present in `tokens`, in Open's order (`TokenKind.allCases`). */
export function presentKinds(
  tokens: Partial<Record<TokenKindName, number>> | null | undefined,
): { kind: TokenKindName; value: number }[] {
  return tokenKindOrder
    .map((kind) => ({ kind, value: tokens?.[kind] ?? 0 }))
    .filter((entry) => entry.value > 0);
}

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

/** What a session's or a day's list-price money is made of (Session's and SessionsDay's `cost`, `pricedCost`, `unpricedTokens`). */
export interface MoneyFacts {
  cost: number | null;
  pricedCost?: number | null;
  unpricedTokens?: number;
}

/** Money at list prices: the whole cost, else what the priced calls cost, with the tokens of the calls that have no list price. */
export interface ListMoney {
  value: number | null;
  unpriced: number;
}

/** A session's or a day's list-price money: its whole cost, else its priced calls' with the rest's tokens. */
export function sessionMoney(session: MoneyFacts): ListMoney {
  if (session.cost !== null) return { value: session.cost, unpriced: 0 };
  return {
    value: session.pricedCost ?? null,
    unpriced: session.unpricedTokens ?? 0,
  };
}

/** What a cost leaves out, as its tooltip says it; undefined when nothing. */
export function unpricedHelp(
  unpriced: number,
  zh = false,
  models: string[] = [],
): string | undefined {
  if (unpriced <= 0) return undefined;
  const count = new Intl.NumberFormat("en").format(unpriced);
  const named = models.length
    ? zh
      ? `：${models.join("、")}`
      : `: ${models.join(", ")}`
    : "";
  return zh
    ? `另有 ${count} 个 Token 来自没有 API 价格的模型，未计入${named}`
    : `${count} more tokens came from models with no API list price and are not included${named}`;
}

interface MessagePage<M> {
  messages?: M[] | null;
  nextMessageCursor?: number | null;
  messagesPending?: boolean;
  messagesComplete?: boolean;
}

/**
 * A session detail with the next page of its messages added. A page carries
 * messages only: the rest stays as the detail had it, the selected turn's
 * calls included, which a page is asked without.
 */
export function withMessagePage<M, D extends MessagePage<M>>(
  detail: D,
  page: MessagePage<M>,
): D {
  return {
    ...detail,
    messages: [...(detail.messages ?? []), ...(page.messages ?? [])],
    nextMessageCursor: page.nextMessageCursor,
    messagesPending: page.messagesPending,
    messagesComplete: page.messagesComplete,
  };
}
