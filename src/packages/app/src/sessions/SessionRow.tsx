import { useMemo, useState, type CSSProperties } from "react";
import type { Session } from "@agent-hud/protocol";
import { moneyAmount } from "@agent-hud/ui/format";
import { stackTurnBars } from "../lib/session-charts";
import { sessionHelp, sessionWhere } from "../lib/sessions";
import { useAnimatedNumber } from "../statistics/StatsTheme";
import { rowColumns, rowGap } from "./SessionListSection";
import { TurnSparkline } from "./TurnSparkline";
import {
  clipped,
  compact,
  moneyText,
  numeric,
  plain,
  row,
  sessionMoney,
  statusLine,
  type Theme,
  unpricedHelp,
} from "./SessionTheme";

/** A row's context: how full, with its bar (amber from 80%), else its tokens. */
function ContextFigure({ session, theme }: { session: Session; theme: Theme }) {
  const zh = Boolean(theme.zh);
  const fill =
    session.contextFill != null
      ? session.contextFill * 100
      : session.contextPercent != null
        ? session.contextPercent
        : null;
  if (fill === null) {
    return session.contextTokens != null ? (
      <span style={{ ...row, gap: 6, justifyContent: "flex-end" }}>
        <span>{zh ? "上下文" : "Context"}</span>
        <span style={{ color: theme.text, ...numeric }}>
          {compact(session.contextTokens)}
        </span>
      </span>
    ) : null;
  }
  return (
    <span
      style={{ ...row, gap: 6, justifyContent: "flex-end" }}
      title={
        zh
          ? "最近一次调用的上下文占模型窗口的比例"
          : "The latest call's context as a share of the model's window"
      }
    >
      <span>{zh ? "上下文" : "Context"}</span>
      <span
        aria-hidden="true"
        style={{
          display: "inline-block",
          width: 40,
          height: 4,
          background: theme.track,
          borderRadius: 4,
          overflow: "hidden",
          flexShrink: 0,
        }}
      >
        <span
          style={{
            display: "block",
            width: `${Math.min(100, Math.max(0, fill))}%`,
            height: "100%",
            background: fill >= 80 ? theme.warning : theme.secondary,
            transition:
              "width 240ms cubic-bezier(0.22, 1, 0.36, 1), background-color 200ms ease",
          }}
        />
      </span>
      <span
        style={{
          color: fill >= 80 ? theme.warning : theme.text,
          minWidth: 28,
          textAlign: "right",
          ...numeric,
        }}
      >
        {Math.round(fill)}%
      </span>
    </span>
  );
}

/**
 * One session as the phone lists it, in three columns: its title, model ·
 * project · client and its status line; its newest turns as a sparkline; its
 * tokens, cost and context. A click opens its page.
 */
export function SessionRow({
  session,
  client,
  theme,
  now,
  onSelect,
  first,
  selected = false,
}: {
  session: Session;
  /** The client that drives the session ("Codex Desktop"). */
  client: string;
  theme: Theme;
  now: number;
  onSelect: () => void;
  first: boolean;
  /** The session the detail beside the list shows. */
  selected?: boolean;
}) {
  const zh = Boolean(theme.zh);
  const [hovered, setHovered] = useState(false);
  const animatedTokens = useAnimatedNumber(session.tokens);
  const bars = useMemo(
    () => (session.turnBars ? stackTurnBars(session.turnBars) : null),
    [session.turnBars],
  );
  const line: CSSProperties = {
    color: theme.secondary,
    fontSize: 11,
    minWidth: 0,
  };
  return (
    <button
      type="button"
      className="session-statistics-row"
      onClick={onSelect}
      aria-current={selected || undefined}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      title={sessionHelp(session, client)}
      style={{
        ...plain,
        width: "100%",
        padding: "10px 14px 9px",
        display: "grid",
        gridTemplateColumns: rowColumns,
        columnGap: rowGap,
        alignItems: "center",
        background: selected
          ? theme.light
            ? "rgba(0,0,0,.06)"
            : "rgba(255,255,255,.09)"
          : hovered
            ? theme.light
              ? "rgba(0,0,0,.03)"
              : "rgba(255,255,255,.04)"
            : "transparent",
        borderTop: first ? 0 : `1px solid ${theme.divider}`,
        transition: "background-color 140ms ease",
      }}
    >
      <span style={{ display: "grid", gap: 4, minWidth: 0 }}>
        <strong style={{ ...clipped, fontSize: 13, fontWeight: 600 }}>
          {session.title || (zh ? "未命名会话" : "Untitled session")}
        </strong>
        <span style={{ ...line, ...clipped }}>
          {sessionWhere(session, client).join(" · ")}
        </span>
        <span style={{ ...line, ...clipped }}>
          {statusLine(session, now, zh)}
        </span>
      </span>
      <span style={{ minWidth: 0 }}>
        {bars && bars.length > 0 ? (
          <TurnSparkline
            bars={bars}
            theme={theme}
            label={
              zh
                ? `最近 ${bars.length} 轮每轮新增的 Token`
                : `New tokens of the last ${bars.length} ${session.turnBars?.by === "period" ? "periods" : "turns"}`
            }
          />
        ) : (
          <span
            style={{
              display: "block",
              height: 34,
              borderBottom: `1px solid ${theme.divider}`,
            }}
          />
        )}
      </span>
      <span
        style={{
          display: "grid",
          gap: 4,
          justifyItems: "end",
          minWidth: 0,
          ...line,
        }}
      >
        <strong
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: theme.text,
            ...numeric,
          }}
        >
          {session.tokens > 0 ? compact(animatedTokens) : "—"}
        </strong>
        <span
          style={numeric}
          title={
            session.accountCost === undefined
              ? unpricedHelp(sessionMoney(session).unpriced, zh)
              : undefined
          }
        >
          {session.accountCost !== undefined
            ? // A priced account's own estimate, in its currency (Open's SessionMoney).
              session.accountCost
              ? `≈${moneyAmount(session.accountCost.amount, session.accountCost.currency, zh ? "zh" : "en", true)}`
              : "—"
            : moneyText(sessionMoney(session), zh)}
        </span>
        <ContextFigure session={session} theme={theme} />
      </span>
    </button>
  );
}
