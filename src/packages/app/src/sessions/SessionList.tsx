import { ChevronRight } from "lucide-react";
import { Card } from "@agent-hud/ui/components/card";
import { Button } from "@agent-hud/ui/components/button";
import type { Session, Snapshot } from "@agent-hud/protocol";
import { sessionClient } from "../lib/sessions";
import type { useSessions } from "../lib/use-sessions";
import { DayHeader } from "./SessionListSection";
import { SessionRow } from "./SessionRow";
import { TodayCard } from "./TodayCard";
import {
  card,
  compact,
  dayStart,
  earlier,
  moneyText,
  row,
  sessionMoney,
  titleForDay,
  type Arrangement,
  type Theme,
} from "./SessionTheme";

/**
 * The Sessions list under its controls: what is loading or failed, today's
 * card, the rows under their days (or the last 24 hours), and more to load.
 */
export function SessionList({
  list,
  arrangement,
  filtering,
  now,
  providers,
  active,
  flipped,
  onFlip,
  onSelect,
  theme,
  selectedId = null,
}: {
  list: ReturnType<typeof useSessions>;
  arrangement: Arrangement;
  /** A search, a project or a source narrows the list. */
  filtering: boolean;
  now: number;
  providers: Snapshot["providers"];
  active: boolean;
  /** The days folded or opened against their default (only today starts open). */
  flipped: Set<number>;
  onFlip: (day: number) => void;
  onSelect: (id: string) => void;
  theme: Theme;
  /** The session shown beside the list, marked in it. */
  selectedId?: string | null;
}) {
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const today = dayStart(now);
  const filtered = list.page?.items ?? [];
  const partial = Boolean(list.page && filtered.length < list.page.total);
  // Core files every matching session under the day of its last message,
  // in the list's order, so each day takes the next of the loaded rows.
  const days = list.page?.days ?? [];
  let next = 0;
  const grouped = days
    .map((day) => {
      const sessions = filtered.slice(next, next + day.sessions);
      next += day.sessions;
      const key = day.start === null ? earlier : Date.parse(day.start);
      return { key, day, sessions };
    })
    .filter((group) => group.sessions.length > 0);
  const todayFigures = days.find(
    (day) => day.start !== null && Date.parse(day.start) === today,
  );
  const cards = (sessions: Session[], noMargin = false) => (
    <Card
      style={{
        ...card(theme, "0"),
        overflow: "hidden",
        gap: 0,
        marginBottom: noMargin ? 0 : 18,
      }}
    >
      {sessions.map((session, index) => (
        <SessionRow
          key={session.id}
          session={session}
          client={sessionClient(session, providers)}
          first={index === 0}
          selected={session.id === selectedId}
          now={now}
          theme={theme}
          onSelect={() => onSelect(session.id)}
        />
      ))}
    </Card>
  );

  return (
    <>
      {!list.page && list.loading && (
        <div
          role="status"
          style={{ color: theme.secondary, fontSize: 12, marginBottom: 12 }}
        >
          {t("Loading sessions…", "正在加载会话…")}
        </div>
      )}
      {list.error && (
        <div
          role="alert"
          style={{
            ...row,
            color: theme.secondary,
            fontSize: 12,
            marginBottom: 12,
          }}
        >
          <span>{list.error}</span>
          <Button variant="ghost" size="sm" onClick={list.retry}>
            {t("Retry", "重试")}
          </Button>
        </div>
      )}
      {partial && (
        <div
          style={{
            color: theme.secondary,
            fontSize: 11,
            marginBottom: 12,
            padding: "0 2px",
          }}
        >
          {zh
            ? `已显示 ${filtered.length} / ${list.page!.total} 个会话。`
            : `Showing ${filtered.length} of ${list.page!.total} sessions.`}
        </div>
      )}
      {todayFigures && (
        <TodayCard day={todayFigures} theme={theme} onSelect={onSelect} />
      )}
      {arrangement === "active"
        ? filtered.length > 0 && (
            <>
              <DayHeader
                title={t("Last 24 hours", "最近 24 小时")}
                count={list.page!.total}
                open
                folded=""
                theme={theme}
                chevron={null}
              />
              {cards(filtered)}
            </>
          )
        : grouped.map((group) => {
            const open = (group.key === today) !== flipped.has(group.key);
            const money = sessionMoney(group.day);
            return (
              <div key={group.key}>
                <DayHeader
                  title={titleForDay(group.key, today, zh)}
                  count={group.day.sessions}
                  open={open}
                  folded={[
                    money.value !== null ? moneyText(money, zh) : null,
                    group.day.tokens > 0 ? compact(group.day.tokens) : "—",
                  ]
                    .filter(Boolean)
                    .join(" · ")}
                  theme={theme}
                  onToggle={() => onFlip(group.key)}
                  chevron={
                    <ChevronRight
                      size={10}
                      style={{
                        transform: open ? "rotate(90deg)" : "rotate(0deg)",
                        alignSelf: "center",
                        flexShrink: 0,
                        transition:
                          "transform 180ms cubic-bezier(0.22, 1, 0.36, 1)",
                      }}
                    />
                  }
                />
                <div
                  style={{
                    display: "grid",
                    gridTemplateRows: open ? "1fr" : "0fr",
                    opacity: open ? 1 : 0,
                    marginBottom: open ? 18 : 0,
                    transition:
                      "grid-template-rows 220ms cubic-bezier(0.22, 1, 0.36, 1), opacity 180ms ease, margin-bottom 220ms ease",
                  }}
                >
                  <div style={{ overflow: "hidden", minHeight: 0 }}>
                    {cards(group.sessions, true)}
                  </div>
                </div>
              </div>
            );
          })}
      {filtered.length === 0 && !list.loading && !list.error && (
        <Card style={card(theme)}>
          <span style={{ color: theme.secondary, fontSize: 12 }}>
            {filtering
              ? t("No matching sessions", "没有匹配的会话")
              : arrangement === "active"
                ? t(
                    "No sessions active in the last 24 hours",
                    "最近 24 小时内无活跃会话",
                  )
                : t("No sessions in the last 7 days", "最近 7 天内无会话")}
          </span>
        </Card>
      )}
      {list.page?.nextOffset !== null && list.page && (
        <div
          style={{
            ...row,
            justifyContent: "center",
            padding: "8px 0 20px",
          }}
        >
          <Button
            variant="outline"
            size="sm"
            disabled={!active || list.loading || list.loadingMore}
            onClick={() => {
              void list.loadMore();
            }}
          >
            {list.loadingMore
              ? t("Loading sessions…", "正在加载会话…")
              : zh
                ? `加载更多（剩余 ${Math.max(0, list.page.total - filtered.length)} 个）`
                : `Load more (${Math.max(0, list.page.total - filtered.length)} remaining)`}
          </Button>
        </div>
      )}
    </>
  );
}
