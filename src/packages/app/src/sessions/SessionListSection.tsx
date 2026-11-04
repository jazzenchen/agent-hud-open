import { useMemo, useState, type CSSProperties, type ReactNode } from "react";
import { Card } from "@agent-hud/ui/components/card";
import {
  sessionHelp,
  sessionKinds,
  sessionWhere,
  stackTurnBars,
  topSessions,
} from "../lib/sessions";
import type { Session } from "@agent-hud/protocol";
import { useAnimatedNumber } from "../statistics/StatsTheme";
import { moneyAmount, share } from "@agent-hud/ui/format";
import { ShareRows, TurnSparkline } from "./SessionCharts";
import {
  card,
  clipped,
  ColorKey,
  compact,
  integer,
  kindColor,
  kindNames,
  Missing,
  moneyText,
  numeric,
  plain,
  row,
  sessionMoney,
  sessionsMoney,
  statusLine,
  type Theme,
  unpricedHelp,
} from "./SessionTheme";

/** The columns a session row and its day's header share: who and how, its turns, its figures. */
export const rowColumns = "minmax(0, 1.3fr) minmax(150px, 1fr) 132px";
export const rowGap = 18;

/**
 * The sessions filed under today in one card (Open's SessionsTodayCard, the
 * phone's TodaySummary): how many, what they cost, their tokens split by the
 * kinds present in Open's order; and beside them, where those tokens went:
 * the sessions that spent the most, each a labelled bar of its share, the
 * rest summed. A session counts its sub-agents, which nest under it.
 */
export function TodayCard({
  sessions,
  theme,
  partial,
  onSelect,
}: {
  sessions: Session[];
  theme: Theme;
  partial: boolean;
  onSelect: (id: string) => void;
}) {
  const zh = Boolean(theme.zh);
  const { total, kinds } = sessionKinds(sessions);
  const animatedTotal = useAnimatedNumber(total);
  const money = sessionsMoney(sessions);
  const kindsTotal = kinds?.reduce((sum, entry) => sum + entry.value, 0) ?? 0;
  const ranked = topSessions(sessions, 5);
  const ranking = [
    ...ranked.top.map((session) => ({
      id: session.id,
      label: session.title || (zh ? "未命名会话" : "Untitled session"),
      value: session.tokens,
      note: share(session.tokens, total),
      color: theme.tertiary,
      title: session.title,
    })),
    ...(ranked.rest.count > 0
      ? [
          {
            id: "__rest__",
            label: (
              <span style={{ color: theme.secondary }}>
                {zh
                  ? `其余 ${ranked.rest.count} 个`
                  : `${ranked.rest.count} more`}
              </span>
            ),
            value: ranked.rest.tokens,
            note: share(ranked.rest.tokens, total),
            color: theme.track,
          },
        ]
      : []),
  ];
  return (
    <Card
      className="session-statistics-today"
      style={{
        ...card(theme, "12px 14px"),
        marginBottom: 18,
        display: "grid",
        gridTemplateColumns: "repeat(auto-fit, minmax(290px, 1fr))",
        columnGap: 0,
        rowGap: 14,
      }}
    >
      <div style={{ minWidth: 0, paddingRight: 20 }}>
        <div
          style={{
            ...row,
            justifyContent: "space-between",
            alignItems: "baseline",
          }}
        >
          <strong style={{ fontSize: 13, fontWeight: 600 }}>
            {zh
              ? `今天 · ${partial ? "已加载 " : ""}${sessions.length} 个会话`
              : `Today · ${sessions.length}${partial ? " loaded " : " "}${sessions.length === 1 ? "session" : "sessions"}`}
          </strong>
          {money.value !== null && (
            <span
              style={{ color: theme.secondary, fontSize: 11 }}
              title={unpricedHelp(money.unpriced, zh)}
            >
              {zh ? "费用 " : "Cost "}
              <span
                style={{
                  fontSize: 12,
                  fontWeight: 600,
                  color: theme.text,
                  ...numeric,
                }}
              >
                {moneyText(money, zh)}
              </span>
            </span>
          )}
        </div>
        <div
          style={{
            ...row,
            alignItems: "baseline",
            gap: 6,
            paddingTop: 4,
            paddingBottom: 10,
          }}
        >
          <strong
            title={
              zh
                ? "今天会话的已记录总 Token 数"
                : "Whole recorded totals of the sessions filed under today"
            }
            style={{
              fontSize: 24,
              fontWeight: 600,
              letterSpacing: "-.4px",
              ...numeric,
            }}
          >
            {compact(animatedTotal)}
          </strong>
          <span style={{ color: theme.secondary, fontSize: 12 }}>
            {zh ? "Token" : "tokens"}
          </span>
        </div>
        {kinds ? (
          <div
            aria-hidden="true"
            style={{
              display: "flex",
              gap: 1.5,
              height: 6,
              overflow: "hidden",
              borderRadius: 3,
              marginBottom: 10,
            }}
          >
            {kinds.map((entry) => (
              <span
                key={entry.kind}
                style={{
                  flex: `${kindsTotal > 0 ? entry.value / kindsTotal : 0} 1 0`,
                  minWidth: 1,
                  background: kindColor(theme, entry.kind),
                  transition: "flex-grow 240ms cubic-bezier(0.22, 1, 0.36, 1)",
                }}
              />
            ))}
          </div>
        ) : (
          <div
            title={
              zh
                ? "所列会话未包含按类型细分的 Token 数据"
                : "Token-kind breakdown is not provided for the listed sessions"
            }
            style={{
              height: 6,
              background: theme.track,
              borderRadius: 3,
              marginBottom: 10,
            }}
          />
        )}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
            columnGap: 20,
            rowGap: 6,
            fontSize: 11,
          }}
        >
          {kinds
            ? kinds.map((entry) => {
                const name = kindNames[entry.kind][zh ? 1 : 0];
                return (
                  <span
                    key={entry.kind}
                    title={`${name} ${integer(entry.value)}`}
                    style={{ ...row, gap: 6 }}
                  >
                    <ColorKey color={kindColor(theme, entry.kind)}>
                      <span style={{ color: theme.secondary }}>{name}</span>
                    </ColorKey>
                    <span
                      style={{
                        marginLeft: "auto",
                        color: theme.text,
                        ...numeric,
                      }}
                    >
                      {compact(entry.value)}
                    </span>
                  </span>
                );
              })
            : (["input", "output", "cacheRead"] as const).map((kind) => (
                <span
                  key={kind}
                  style={{ ...row, gap: 6, color: theme.secondary }}
                >
                  {kindNames[kind][zh ? 1 : 0]}
                  <Missing theme={theme} />
                </span>
              ))}
        </div>
      </div>
      {ranking.length > 0 && (
        <div
          style={{
            minWidth: 0,
            paddingLeft: 20,
            borderLeft: `1px solid ${theme.divider}`,
          }}
        >
          <div
            style={{ ...row, justifyContent: "space-between", marginBottom: 9 }}
          >
            <strong style={{ fontSize: 13, fontWeight: 600 }}>
              {zh ? "今天的 Token 花在哪" : "Where today's tokens went"}
            </strong>
            <span style={{ color: theme.secondary, fontSize: 11 }}>
              {zh ? "占比" : "Share"}
            </span>
          </div>
          <ShareRows
            rows={ranking}
            whole={total}
            theme={theme}
            onSelect={(id) => {
              if (id !== "__rest__") onSelect(id);
            }}
          />
        </div>
      )}
    </Card>
  );
}

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
}: {
  session: Session;
  /** The client that drives the session ("Codex Desktop"). */
  client: string;
  theme: Theme;
  now: number;
  onSelect: () => void;
  first: boolean;
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
        background: hovered
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

/** A day's header over its rows, on the rows' columns: its name and counts, then what the columns hold or, folded, what its sessions spent. */
export function DayHeader({
  title,
  count,
  open,
  folded,
  theme,
  onToggle,
  chevron,
}: {
  title: string;
  count: number;
  open: boolean;
  /** What the day spent, shown while it is folded. */
  folded: string;
  theme: Theme;
  /** Folds or opens the day; a list without days (Active) has nothing to fold. */
  onToggle?: () => void;
  chevron: ReactNode;
}) {
  const zh = Boolean(theme.zh);
  const Element = onToggle ? "button" : "div";
  return (
    <Element
      {...(onToggle
        ? { type: "button" as const, onClick: onToggle, "aria-expanded": open }
        : {})}
      style={{
        ...plain,
        cursor: onToggle ? "pointer" : "default",
        width: "100%",
        display: "grid",
        gridTemplateColumns: rowColumns,
        columnGap: rowGap,
        alignItems: "baseline",
        padding: open ? "0 15px" : "9px 15px",
        borderRadius: 10,
        border: open ? "1px solid transparent" : `1px solid ${theme.border}`,
        background: open ? "transparent" : theme.card,
        marginBottom: open ? 6 : 8,
        color: theme.secondary,
        transition:
          "background-color 180ms ease, border-color 180ms ease, padding 180ms ease",
      }}
    >
      <span style={{ ...row, gap: 6, alignItems: "baseline" }}>
        {chevron}
        <strong style={{ fontSize: 13, fontWeight: 600, color: theme.text }}>
          {title}
        </strong>
        <span style={{ fontSize: 11, ...numeric }}>{count}</span>
      </span>
      <span
        style={{
          fontSize: 11,
          opacity: open ? 1 : 0,
          transition: "opacity 180ms ease",
          ...clipped,
        }}
      >
        {zh ? "每轮新增 Token" : "New tokens per turn"}
      </span>
      <span
        style={{ fontSize: 11, textAlign: "right", ...numeric, ...clipped }}
      >
        {open
          ? zh
            ? "Token · 费用 · 上下文"
            : "Tokens · cost · context"
          : folded}
      </span>
    </Element>
  );
}
