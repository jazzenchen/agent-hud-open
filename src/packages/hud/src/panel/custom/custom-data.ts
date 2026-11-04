/* What a control of an agent's own reads: the snapshot as the collections the
 * catalog names (presentation/tree/vocabulary.rs), the items a repeat picks,
 * and its text with each {item.field} placeholder filled in. The app checked
 * the tree before saving it, so a path that names nothing reads as nothing. */
import type { Quota, Snapshot } from "@agent-hud/protocol";
import {
  age,
  countdown,
  moneyShort,
  numberFormat,
  percent,
  tokensShort,
  type Language,
} from "@agent-hud/ui/format";
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

/** What a placeholder can read where it stands. */
export interface Scope {
  /** The items of the repeats around it, innermost last. */
  items: { name: string; item: Item }[];
  state: Record<string, string>;
  now: number;
  language: Language;
}

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

/** The value a path reads where it stands: `window.used`, `now.time`, `state.tab`. */
export function read(path: string, scope: Scope): unknown {
  const dot = path.indexOf(".");
  if (dot < 0) return undefined;
  const name = path.slice(0, dot).trim(),
    field = path.slice(dot + 1).trim();
  if (name === "now") return field === "time" ? scope.now : undefined;
  if (name === "state") return scope.state[field];
  for (let i = scope.items.length - 1; i >= 0; i--)
    if (scope.items[i].name === name) return scope.items[i].item[field];
  return undefined;
}

/** The format a field reads in when none is picked. */
function ownFormat(path: string): string | null {
  const field = path.slice(path.indexOf(".") + 1);
  if (path === "now.time") return "clock";
  if (field === "used" || field === "burn") return "percent";
  if (field === "resetsAt" || field === "runsOutAt") return "countdown";
  if (field === "updatedAt") return "relative";
  if (field === "amount" || field === "cost") return "money";
  if (field === "tokens") return "tokens";
  return null;
}

function time(value: unknown): number | null {
  if (typeof value === "number") return value;
  if (typeof value !== "string") return null;
  const at = Date.parse(value);
  return Number.isFinite(at) ? at : null;
}

const locale = (language: Language) => (language === "zh" ? "zh-CN" : "en-US");

/** One value as text, in `format` or the field's own. */
export function formatted(
  value: unknown,
  path: string,
  format: string | null,
  scope: Scope,
): string {
  if (value == null) return "—";
  const as = format ?? ownFormat(path);
  const number = typeof value === "number" ? value : Number(value);
  switch (as) {
    case "percent": {
      if (!Number.isFinite(number)) return "—";
      const text = percent(number * 100);
      return path.endsWith(".burn") ? `${text}/h` : text;
    }
    case "number":
      return Number.isFinite(number)
        ? numberFormat(locale(scope.language), {
            maximumFractionDigits: 2,
          }).format(number)
        : String(value);
    case "money": {
      if (!Number.isFinite(number)) return "—";
      const currency = read(
        `${scope.items.at(-1)?.name ?? ""}.currency`,
        scope,
      );
      try {
        return moneyShort(
          number,
          typeof currency === "string" ? currency : "USD",
          scope.language,
        );
      } catch {
        return `${number.toFixed(2)} ${currency ?? ""}`.trim();
      }
    }
    case "tokens":
      return Number.isFinite(number) ? tokensShort(number) : "—";
    case "countdown": {
      const at = time(value);
      return at == null ? "—" : countdown((at - scope.now) / 1000);
    }
    case "relative": {
      const at = time(value);
      return at == null ? "—" : age((scope.now - at) / 1000, scope.language);
    }
    case "clock":
    case "date":
    case "weekday": {
      const at = time(value);
      if (at == null) return "—";
      const options: Intl.DateTimeFormatOptions =
        as === "clock"
          ? { hour: "numeric", minute: "2-digit" }
          : as === "date"
            ? { month: "short", day: "numeric" }
            : { weekday: "long" };
      return new Intl.DateTimeFormat(locale(scope.language), options).format(
        at,
      );
    }
    default:
      return typeof value === "string" ? value : String(value);
  }
}

/** A text with each {item.field} or {item.field|format} filled in. */
export function fill(text: string, scope: Scope): string {
  return text.replace(/\{([^{}]*)\}/g, (_, inner: string) => {
    const [path, format] = inner.split("|").map((part) => part.trim());
    return formatted(read(path, scope), path, format || null, scope);
  });
}

/** A field that is one placeholder reads its raw value; any other text is filled in. */
export function valueOf(field: unknown, scope: Scope): unknown {
  if (typeof field !== "string") return field;
  const one = /^\{([^{}|]*)\}$/.exec(field.trim());
  return one ? read(one[1].trim(), scope) : fill(field, scope);
}

/** The items a repeat or a ring draws: those matching `where`, sorted, at most `limit`. */
export function pick(
  all: Collections,
  over: string,
  where: Record<string, unknown> | undefined,
  sort: string | undefined,
  limit: number | undefined,
  scope: Scope,
): Item[] {
  const items = all[over as keyof Collections] ?? [];
  const wanted = Object.entries(where ?? {}).map(
    ([field, value]) =>
      [
        field,
        (Array.isArray(value) ? value : [value]).map((v) => valueOf(v, scope)),
      ] as const,
  );
  const matching = items.filter((item) =>
    wanted.every(([field, values]) =>
      values.some((value) =>
        field === "agent"
          ? sameAgent(item[field], value)
          : item[field] === value ||
            (item[field] != null && String(item[field]) === String(value)),
      ),
    ),
  );
  if (sort) {
    const descending = sort.startsWith("-"),
      field = sort.replace(/^-/, "");
    const key = (item: Item) => {
      const value = item[field];
      return typeof value === "number" ? value : (time(value) ?? value);
    };
    matching.sort((a, b) => {
      const x = key(a),
        y = key(b);
      if (x == null) return y == null ? 0 : 1;
      if (y == null) return -1;
      const order =
        typeof x === "number" && typeof y === "number"
          ? x - y
          : String(x).localeCompare(String(y));
      return descending ? -order : order;
    });
  }
  return matching.slice(0, Math.max(1, Math.min(limit ?? 20, 20)));
}

/** An item's name inside a repeat over `over`. */
export const itemName = (over: string) =>
  NAME_OF[over as keyof Collections] ?? over;

/** Whether a `when` holds where it stands. */
export function holds(
  when: Record<string, unknown> | undefined,
  scope: Scope,
): boolean {
  if (!when) return true;
  const value = valueOf(when.value, scope);
  const equal = (other: unknown) =>
    value === other ||
    (value != null && other != null && String(value) === String(other));
  if ("is" in when) return equal(when.is);
  if ("not" in when) return !equal(when.not);
  if ("in" in when)
    return Array.isArray(when.in) && when.in.some((other) => equal(other));
  if ("above" in when)
    return typeof value === "number" && value > Number(when.above);
  if ("below" in when)
    return typeof value === "number" && value < Number(when.below);
  if ("present" in when)
    return (value != null && value !== "") === Boolean(when.present);
  return true;
}

/** Whether a tree reads the time, so it is drawn again as the minute turns. */
export function readsTime(node: unknown): boolean {
  return JSON.stringify(node ?? null).includes("now.time");
}

/** The states a control placed in the panel declares, at their defaults. */
export function initialState(node: {
  state?: Record<string, { options?: string[]; default?: string }>;
}): Record<string, string> {
  return Object.fromEntries(
    Object.entries(node.state ?? {}).map(([name, state]) => [
      name,
      state.default ?? state.options?.[0] ?? "",
    ]),
  );
}

/** The option after the current one, round to the first. */
export function nextOption(
  node: { state?: Record<string, { options?: string[] }> },
  name: string,
  current: string | undefined,
): string | undefined {
  const options = node.state?.[name]?.options ?? [];
  if (!options.length) return current;
  const index = options.indexOf(current ?? "");
  return options[(index + 1) % options.length];
}
