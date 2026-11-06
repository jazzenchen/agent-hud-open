import { Card } from "@agent-hud/ui/components/card";
import type { SessionsDay } from "@agent-hud/protocol";
import { share } from "@agent-hud/ui/format";
import { presentKinds } from "../lib/sessions";
import { Tween } from "../statistics/motion";
import { moneyAt, reveal, tokensAt, useBarReach } from "./SessionMotion";
import { ShareRows } from "./ShareRows";
import {
  card,
  ColorKey,
  integer,
  kindColor,
  kindLabel,
  Missing,
  numeric,
  row,
  sessionMoney,
  type Theme,
  unpricedHelp,
} from "./SessionTheme";

/**
 * The sessions filed under today in one card (Open's SessionsTodayCard, the
 * phone's TodaySummary), as Core counts every matching one: how many, what
 * they cost, their tokens split by the kinds present in Open's order; and
 * beside them, where those tokens went: the sessions that spent the most,
 * each a labelled bar of its share, the rest summed. A session counts its
 * sub-agents, which nest under it. It fades in when first read; its figures
 * count up and its bars fill, then move as the day goes on.
 */
export function TodayCard({
  day,
  theme,
  onSelect,
}: {
  day: SessionsDay;
  theme: Theme;
  onSelect: (id: string) => void;
}) {
  const zh = Boolean(theme.zh);
  const total = day.tokens;
  const kinds = day.usage ? presentKinds(day.usage) : null;
  const reach = useBarReach();
  const money = sessionMoney(day);
  const kindsTotal = kinds?.reduce((sum, entry) => sum + entry.value, 0) ?? 0;
  const ranking = [
    ...day.top.map((session) => ({
      id: session.id,
      label: session.title || (zh ? "未命名会话" : "Untitled session"),
      value: session.tokens,
      note: (value: number) => share(value, total),
      color: theme.tertiary,
      title: session.title,
    })),
    ...(day.rest.count > 0
      ? [
          {
            id: "__rest__",
            label: (
              <span style={{ color: theme.secondary }}>
                {zh ? `其余 ${day.rest.count} 个` : `${day.rest.count} more`}
              </span>
            ),
            value: day.rest.tokens,
            note: (value: number) => share(value, total),
            color: theme.track,
          },
        ]
      : []),
  ];
  return (
    <Card
      className={`session-statistics-today ${reveal}`}
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
              ? `今天 · ${day.sessions} 个会话`
              : `Today · ${day.sessions} ${day.sessions === 1 ? "session" : "sessions"}`}
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
                <Tween value={money.value} format={moneyAt(money, zh)} />
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
            <Tween value={total} format={tokensAt} />
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
              clipPath: reach,
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
                const name = kindLabel(entry.kind, zh);
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
                      <Tween value={entry.value} format={tokensAt} />
                    </span>
                  </span>
                );
              })
            : (["input", "output", "cacheRead"] as const).map((kind) => (
                <span
                  key={kind}
                  style={{ ...row, gap: 6, color: theme.secondary }}
                >
                  {kindLabel(kind, zh)}
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
