// The snapshot as the collections the catalog names
// (presentation/tree/vocabulary.rs), which a control's repeats go over.
import type { Quota, Snapshot } from "@agent-hud/protocol";
import { agentArtworkKey } from "../../agent-artwork.ts";

/** One item of a collection: the catalog's fields, plus what a control needs to
 * draw or open it (`quota` for rings, `session` for a click). */
export type Item = Record<string, unknown> & {
  quota?: Quota;
  session?: Snapshot["sessions"][number];
};
export type Collections = Record<
  "windows" | "balances" | "accounts" | "agents" | "sessions",
  Item[]
>;
/** The collection each item name belongs to. */
export const ITEM_OF: Record<string, keyof Collections> = {
  window: "windows",
  balance: "balances",
  account: "accounts",
  agent: "agents",
  session: "sessions",
};
const NAME_OF = Object.fromEntries(
  Object.entries(ITEM_OF).map(([item, name]) => [name, item]),
) as Record<keyof Collections, string>;

const share = (used: number | null | undefined) =>
  used == null || !Number.isFinite(used) ? null : used / 100;

/** The kind of a window, the same on every computer: its id without its account. */
export function windowKind(quota: Quota): string {
  const account = quota.accountId ?? "";
  if (account && quota.id.startsWith(`${account}:`))
    return quota.id.slice(account.length + 1);
  const legacy = quota.legacyWindowId ?? "";
  return legacy.includes("/")
    ? legacy.slice(legacy.indexOf("/") + 1)
    : quota.id;
}

/** The snapshot as the collections a repeat goes over. */
export function collections(snapshot: Snapshot): Collections {
  const windows = snapshot.quotas.map((quota): Item => ({
    id: quota.id,
    agent: quota.vendor,
    account: quota.accountId ?? quota.account ?? null,
    accountName: quota.account,
    kind: windowKind(quota),
    name: quota.name,
    short: quota.shortLabel ?? quota.name,
    used: share(quota.usedPercent),
    level: quota.status,
    resetsAt: quota.resetAt,
    burn: share(quota.metrics?.burnRatePctPerHour),
    runsOutAt:
      quota.metrics?.outlook?.kind === "exhausts"
        ? (quota.metrics.outlook.exhaustsAt ?? null)
        : null,
    quota,
  }));
  const balances = snapshot.billing.map((balance): Item => {
    const day = balance.metrics?.periods?.day;
    return {
      id: balance.id,
      agent: balance.vendor,
      account: balance.accountId ?? balance.id,
      name: balance.label,
      amount: balance.amount,
      currency: balance.currency,
      level: balance.level ?? "normal",
      cost: typeof day?.cost === "number" ? day.cost : null,
      runsOutAt: day?.runsOutAt ?? null,
    };
  });
  const accounts = (snapshot.accounts ?? []).map((account): Item => ({
    id: account.id,
    agent: account.provider,
    name: account.label ?? account.name,
    plan: account.planLabel ?? account.plan ?? null,
  }));
  const agents = snapshot.providers
    .filter((provider) => provider.detected && provider.enabled)
    .map((provider): Item => {
      const own = snapshot.sessions.filter((session) =>
        sameAgent(session.source, provider.id),
      );
      return {
        id: provider.id,
        name: provider.name,
        sessions: own.length,
        running: own.filter((session) => session.state === "running").length,
      };
    });
  const sessions = snapshot.sessions.map((session): Item => ({
    id: session.id,
    agent: session.source,
    title: session.title,
    project: session.project ?? null,
    state: session.state,
    updatedAt: session.lastMessageAt ?? session.updatedAt,
    tokens: session.tokens,
    cost: session.cost ?? null,
    model: session.modelName ?? session.model ?? null,
    session,
  }));
  return { windows, balances, accounts, agents, sessions };
}

/** Whether two names are one agent: "claude-code" and "claude", "codex" and "openai". */
export function sameAgent(a: unknown, b: unknown): boolean {
  return (
    typeof a === "string" &&
    typeof b === "string" &&
    agentArtworkKey(a) === agentArtworkKey(b)
  );
}

/** An item's name inside a repeat over `over`. */
export const itemName = (over: string) =>
  NAME_OF[over as keyof Collections] ?? over;
