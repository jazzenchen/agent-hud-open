import { useMemo, useState, type CSSProperties } from "react";
import type { Session } from "@agent-hud/protocol";
import { stackTurnBars } from "../lib/session-charts";
import { sessionHelp, sessionWhere } from "../lib/sessions";
import { Tween, useTween } from "../statistics/motion";
import { useRowGrid } from "./SessionListSection";
import { accountAt, moneyAt, reveal, tokensAt } from "./SessionMotion";
import { TurnSparkline } from "./TurnSparkline";
import {
  clipped,
  moneyText,
  numeric,
  plain,
  row,
  sessionMoney,
  statusLine,
  type Theme,
  unpricedHelp,
} from "./SessionTheme";
import { useFormat, useT } from "@agent-hud/ui/format-context";

/** A row's context: how full, with its bar (amber from 80%), else its tokens. */
function ContextFigure({ session, theme }: { session: Session; theme: Theme }) {
  const t = useT();
  const fill =
    session.contextFill != null
      ? session.contextFill * 100
      : session.contextPercent != null
        ? session.contextPercent
        : null;
  // The bar and its figure move together, from empty when first drawn.
  const shown = useTween(fill ?? 0);
  if (fill === null) {
    return session.contextTokens != null ? (
      <span style={{ ...row, gap: 6, justifyContent: "flex-end" }}>
        <span>{t("Context")}</span>
        <span style={{ color: theme.text, ...numeric }}>
          <Tween value={session.contextTokens} format={tokensAt} />
        </span>
      </span>
    ) : null;
  }
  return (
    <span
      style={{ ...row, gap: 6, justifyContent: "flex-end" }}
      title={t("The latest call's context as a share of the model's window")}
    >
      <span>{t("Context")}</span>
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
            width: `${Math.min(100, Math.max(0, shown))}%`,
            height: "100%",
            background: fill >= 80 ? theme.warning : theme.secondary,
            transition: "background-color 200ms ease",
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
        {Math.round(shown)}%
      </span>
    </span>
  );
}

/**
 * One session as the phone lists it, in three columns: its title, model ·
 * project · client and its status line; its newest turns as a sparkline; its
 * tokens, cost and context. A click opens its page. Its figures move to new
 * values; a row that arrives after its card was drawn fades in.
 */
export function SessionRow({
  session,
  client,
  theme,
  now,
  onSelect,
  first,
  selected = false,
  arrived = false,
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
  /** The row came after its card was drawn (a new session, a page loaded). */
  arrived?: boolean;
}) {
  const t = useT();
  const { language } = useFormat();
  const [hovered, setHovered] = useState(false);
  const [fresh] = useState(arrived);
  const grid = useRowGrid();
  const money = sessionMoney(session);
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
      className={`session-statistics-row${fresh ? ` ${reveal}` : ""}`}
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
        gridTemplateColumns: grid.columns,
        columnGap: grid.gap,
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
          {session.title || t("Untitled session")}
        </strong>
        <span style={{ ...line, ...clipped }}>
          {sessionWhere(session, client).join(" · ")}
        </span>
        <span style={{ ...line, ...clipped }}>
          {statusLine(session, now, language)}
        </span>
      </span>
      <span style={{ minWidth: 0 }}>
        {bars && bars.length > 0 ? (
          <TurnSparkline
            bars={bars}
            theme={theme}
            label={
              session.turnBars?.by === "period"
                ? t("New tokens of the last {{count}} periods", {
                    count: bars.length,
                  })
                : t("New tokens of the last {{count}} turns", {
                    count: bars.length,
                  })
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
          {session.tokens > 0 ? (
            <Tween value={session.tokens} format={tokensAt} />
          ) : (
            "—"
          )}
        </strong>
        <span
          style={numeric}
          title={
            session.accountCost === undefined
              ? unpricedHelp(money.unpriced, language)
              : undefined
          }
        >
          {session.accountCost !== undefined ? (
            // A priced account's own estimate, in its currency (Open's SessionMoney).
            session.accountCost ? (
              <Tween
                value={session.accountCost.amount}
                format={accountAt(session.accountCost, language)}
              />
            ) : (
              "—"
            )
          ) : money.value !== null ? (
            <Tween value={money.value} format={moneyAt(money, language)} />
          ) : (
            moneyText(money, language)
          )}
        </span>
        <ContextFigure session={session} theme={theme} />
      </span>
    </button>
  );
}
