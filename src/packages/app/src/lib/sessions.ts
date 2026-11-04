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
