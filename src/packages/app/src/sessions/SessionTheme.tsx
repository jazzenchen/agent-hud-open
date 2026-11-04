import type { CSSProperties, ReactNode } from "react";
import {
  Select,
  SelectContent,
  SelectTrigger,
  SelectValue,
} from "@agent-hud/ui/components/select";
import type { Session, SessionDetail, Snapshot } from "@agent-hud/protocol";
import {
  age,
  dateFormat,
  moneyAmount,
  snapshotLanguage,
} from "@agent-hud/ui/format";
import {
  lastMessageTime,
  type ListMoney,
  type TokenKindName,
} from "../lib/sessions";
import {
  themeFor as statsTheme,
  type StatsTheme,
} from "../statistics/StatsTheme";
export { sessionMoney, unpricedHelp, type ListMoney } from "../lib/sessions";
export {
  compact,
  integer,
  kindLabel,
  number as numeric,
  truncate as clipped,
} from "../statistics/StatsTheme";

export type Arrangement = "day" | "active";
export type Entry = SessionDetail["turns"][number];

/** The Statistics window's colours, and the Sessions page's own. */
export interface Theme extends StatsTheme {
  input: string;
  output: string;
  cache: string;
  cacheWrite: string;
  reasoning: string;
  /** The cost bars and their key. */
  accent: string;
  /** A context near its limit; a cache sent again. */
  warning: string;
  /** Where the client compacted the conversation. */
  compaction: string;
  /** A link: Reveal log. */
  action: string;
  /** A picked column behind the bars. */
  pick: string;
  /** A card's colour as one solid, for halos behind chart labels. */
  surface: string;
}

export const ALL = "__all__";
export const earlier = Number.NEGATIVE_INFINITY;

/** What a session's calls would cost at API list prices, in US dollars; always an estimate. A partial one, which leaves out calls with no list price, ends in "+". */
export const cost = (value: number | null, zh = false, partial = false) =>
  value === null
    ? "—"
    : value === 0 && !partial
      ? // Calls to a model on this computer cost nothing.
        zh
        ? "免费"
        : "Free"
      : `≈${moneyAmount(value, "USD", zh ? "zh" : "en")}${partial ? "+" : ""}`;

/** `money` as a cost, "+" when calls with no list price are left out. */
export const moneyText = (money: ListMoney, zh = false) =>
  cost(money.value, zh, money.unpriced > 0);

export const row: CSSProperties = {
  display: "flex",
  alignItems: "center",
  minWidth: 0,
  gap: 8,
};

export const plain: CSSProperties = {
  padding: 0,
  border: 0,
  borderRadius: 0,
  background: "transparent",
  color: "inherit",
  textAlign: "left",
  font: "inherit",
  cursor: "pointer",
};

/** The Sessions page's colours in the window's look (`light`, App's theme), as Open's `Theme.forScheme`. */
export function themeFor(snapshot: Snapshot, light: boolean): Theme {
  const base = statsTheme(light, snapshotLanguage(snapshot) === "zh");
  return light
    ? {
        ...base,
        input: "#398ad6",
        output: "#c26e12",
        cache: "#00a091",
        cacheWrite: "#9470cd",
        reasoning: "#c35f92",
        accent: "#1f9d55",
        warning: "#b7791f",
        compaction: "#d97706",
        action: "#0a6cdf",
        pick: "rgba(0,0,0,.05)",
        surface: "#fff",
      }
    : {
        ...base,
        input: "#6db0f4",
        output: "#e29858",
        cache: "#31c3b5",
        cacheWrite: "#b699eb",
        reasoning: "#e38ab5",
        accent: "#4ade80",
        warning: "#eab308",
        compaction: "#f59e0b",
        action: "#0a84ff",
        pick: "rgba(255,255,255,.07)",
        surface: "#333",
      };
}

/** A kind's colour on the page (Open's `Theme.kind`). */
export function kindColor(theme: Theme, kind: TokenKindName): string {
  return kind === "cacheRead" ? theme.cache : theme[kind];
}

export function card(theme: Theme, padding = "12px 14px"): CSSProperties {
  return {
    borderRadius: 10,
    border: `1px solid ${theme.border}`,
    boxShadow: "none",
    padding,
    gap: 8,
    minWidth: 0,
    color: "inherit",
    background: `var(--statistics-card, ${theme.card})`,
  };
}

export function dayStart(at: number) {
  const date = new Date(at);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** How long ago the session's last message was written ("2h ago"). */
export function sinceLabel(session: Session, now: number, zh = false) {
  const at = Date.parse(lastMessageTime(session));
  return Number.isFinite(at) ? age((now - at) / 1000, zh ? "zh" : "en") : "—";
}

/** A row's status line (Open's `SessionCard.status`): the turns, the time. */
export function statusLine(session: Session, now: number, zh = false): string {
  const turns =
    session.turnCount != null && session.turnCount > 0
      ? zh
        ? `${session.turnCount} 轮`
        : `${session.turnCount} ${session.turnCount === 1 ? "turn" : "turns"}`
      : null;
  return [turns, sinceLabel(session, now, zh)].filter(Boolean).join(" · ");
}

export function titleForDay(day: number, today: number, zh = false) {
  if (day === earlier) return zh ? "更早" : "Earlier";
  if (day === today) return zh ? "今天" : "Today";
  const yesterday = new Date(today);
  yesterday.setDate(yesterday.getDate() - 1);
  if (day === yesterday.getTime()) return zh ? "昨天" : "Yesterday";
  return dateFormat(zh ? "zh-CN" : "en-GB", {
    weekday: "long",
    month: "long",
    day: "numeric",
  }).format(new Date(day));
}

export function timeLabel(at: string, zh = false) {
  const time = Date.parse(at);
  return Number.isFinite(time)
    ? dateFormat(zh ? "zh-CN" : "en-GB", {
        weekday: "short",
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
      }).format(new Date(time))
    : "—";
}

export function durationLabel(
  startMs: number,
  endMs: number,
  zh = false,
): string {
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs < startMs)
    return "—";
  const seconds = Math.max(0, Math.round((endMs - startMs) / 1000));
  if (seconds < 60) return zh ? `${seconds} 秒` : `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return zh ? `${minutes} 分钟` : `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const remMin = minutes % 60;
  return zh
    ? remMin > 0
      ? `${hours} 小时 ${remMin} 分钟`
      : `${hours} 小时`
    : remMin > 0
      ? `${hours}h ${remMin}m`
      : `${hours}h`;
}

export function projectName(path: string | null, zh = false) {
  return path
    ? path
        .replace(/[\\/]+$/, "")
        .split(/[\\/]/)
        .at(-1) || path
    : zh
      ? "未归属项目"
      : "Unassigned";
}

export function Missing({
  theme,
  title = "This field is not included in the session data.",
}: {
  theme: Theme;
  title?: string;
}) {
  return (
    <span title={title} style={{ color: theme.secondary }}>
      —
    </span>
  );
}

export function ColorKey({
  color,
  children,
}: {
  color: string;
  children: ReactNode;
}) {
  return (
    <span style={{ ...row, display: "inline-flex", gap: 5 }}>
      <span
        aria-hidden="true"
        style={{
          width: 7,
          height: 7,
          borderRadius: 2,
          background: color,
          flexShrink: 0,
        }}
      />
      {children}
    </span>
  );
}

export function SourceSelect({
  value,
  onChange,
  width,
  label,
  children,
  theme,
}: {
  value: string;
  onChange: (value: string) => void;
  width: number;
  label: string;
  children: ReactNode;
  theme: Theme;
}) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger
        aria-label={label}
        size="sm"
        style={{
          width,
          minWidth: 0,
          height: 26,
          padding: "2px 8px",
          fontSize: 11,
          borderRadius: 6,
          borderColor: theme.border,
          color: theme.text,
          background: theme.card,
          boxShadow: "none",
        }}
      >
        <SelectValue />
      </SelectTrigger>
      <SelectContent>{children}</SelectContent>
    </Select>
  );
}
