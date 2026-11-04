import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { ChevronLeft, ChevronRight, Search, X } from "lucide-react";
import { Card } from "@agent-hud/ui/components/card";
import { Button } from "@agent-hud/ui/components/button";
import { SelectItem } from "@agent-hud/ui/components/select";
import { Input } from "@agent-hud/ui/components/input";
import { lastMessageTime, sessionClient } from "./lib/sessions";
import { useSessionDetail, useSessions } from "./lib/use-sessions";
import type {
  NativeMethod,
  Session,
  SessionsQuery,
  Snapshot,
} from "@agent-hud/protocol";
import { InlineDetail } from "./sessions/SessionDetailView";
import { Segments } from "./statistics/Segments";
import {
  DayHeader,
  SessionRow,
  TodayCard,
} from "./sessions/SessionListSection";
import {
  ALL,
  card,
  clipped,
  compact,
  dayStart,
  earlier,
  moneyText,
  numeric,
  plain,
  projectName,
  row,
  sessionsMoney,
  SourceSelect,
  themeFor,
  titleForDay,
  type Arrangement,
  type SessionGroup,
} from "./sessions/SessionTheme";

interface SessionStatisticsProps {
  snapshot: Snapshot;
  /** The window's look (App's theme): with Match screen on, the look of the screen the window is on. */
  light: boolean;
  active?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  initialSessionId?: string | null;
  onClearSelection?: () => void;
}

/** The native Sessions page, including its own inline detail selection. */
export default function SessionStatistics({
  snapshot,
  light,
  action,
  active = true,
  initialSessionId = null,
  onClearSelection,
}: SessionStatisticsProps) {
  const [arrangement, setArrangement] = useState<Arrangement>("day");
  const [source, setSource] = useState(ALL);
  const [project, setProject] = useState(ALL);
  const [query, setQuery] = useState("");
  const [flipped, setFlipped] = useState<Set<number>>(() => new Set());
  const [selectedId, setSelectedId] = useState<string | null>(initialSessionId);
  const [selectedEntry, setSelectedEntry] = useState<string | null>(null);
  const actionRef = useRef(action);
  actionRef.current = action;
  useEffect(() => {
    if (initialSessionId) setSelectedId(initialSessionId);
  }, [initialSessionId]);
  const theme = themeFor(snapshot, light);
  const zh = Boolean(theme.zh);
  const t = (en: string, cn: string) => (zh ? cn : en);
  const now = Date.parse(snapshot.generatedAt),
    today = dayStart(now);
  const listQuery = useMemo<Omit<SessionsQuery, "offset" | "limit">>(
    () => ({
      scope: arrangement === "active" ? "active" : "recent",
      rootsOnly: true,
      // Rows draw their turns and context, as the phone's do.
      turnBars: true,
      ...(source === ALL ? {} : { source }),
      ...(project === ALL
        ? {}
        : { project: project === "__unassigned__" ? null : project }),
      ...(query.trim() ? { search: query.trim() } : {}),
    }),
    [arrangement, source, project, query],
  );
  const revision =
    snapshot.sessionsRevision ?? (snapshot.mode === "demo" ? now : 0);
  const list = useSessions(listQuery, revision, active, action);
  const filtered = list.page?.items ?? [];
  const sources = list.page?.sources ?? [];
  const projects = (list.page?.projects ?? []).filter(
    (path): path is string => path !== null,
  );
  const partial = Boolean(list.page && filtered.length < list.page.total);
  const observed =
    snapshot.sessions.find((session) => session.id === selectedId) ??
    filtered.find((session) => session.id === selectedId);
  const {
    detail,
    loading,
    error,
    setError,
    retry: retryDetail,
    loadMessages,
    loadingMessages,
  } = useSessionDetail(selectedId, observed, active, action, selectedEntry);
  const selected = observed ?? detail?.session;
  const parent =
    detail?.parent !== undefined
      ? detail.parent
      : selected?.parentId
        ? (snapshot.sessions.find(
            (session) => session.id === selected.parentId,
          ) ?? filtered.find((session) => session.id === selected.parentId))
        : null;
  useEffect(() => setSelectedEntry(null), [selectedId]);

  useEffect(() => {
    if (!selectedId || !active) return;
    const back = (event: KeyboardEvent) => {
      if (event.metaKey && event.key === "[") {
        event.preventDefault();
        setSelectedId(parent?.id ?? null);
        if (!parent) onClearSelection?.();
      }
    };
    window.addEventListener("keydown", back);
    return () => window.removeEventListener("keydown", back);
  }, [selectedId, parent?.id, onClearSelection, active]);
  const groups = new Map<number, Session[]>();
  const firstNamed = new Date(today);
  firstNamed.setDate(firstNamed.getDate() - 6);
  // A session files under the day of its last message, whatever it is doing.
  for (const session of filtered) {
    const observedAt = Date.parse(lastMessageTime(session));
    const day = dayStart(Math.min(observedAt, now));
    const key =
      day < firstNamed.getTime() || !Number.isFinite(day) ? earlier : day;
    const group = groups.get(key);
    if (group) group.push(session);
    else groups.set(key, [session]);
  }
  const grouped: SessionGroup[] = [...groups]
    .map(([day, sessions]) => ({ day, sessions }))
    .sort((a, b) => b.day - a.day);
  const todaySessions = groups.get(today) ?? [];
  const choose = (id: string) => {
    setSelectedId(id);
    setSelectedEntry(null);
  };
  const contentStyle: CSSProperties = {
    color: `var(--statistics-text, ${theme.text})`,
    background: `var(--statistics-ground, ${theme.ground})`,
    fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif',
    fontSize: 13,
    lineHeight: 1.35,
    minWidth: 0,
    padding: "4px 22px 24px",
  };
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
          client={sessionClient(session, snapshot.providers)}
          first={index === 0}
          now={now}
          theme={theme}
          onSelect={() => choose(session.id)}
        />
      ))}
    </Card>
  );

  return (
    <section
      className="session-statistics-native"
      style={contentStyle}
      aria-label={t("Session statistics", "会话统计")}
    >
      {selectedId ? (
        <>
          <div
            style={{
              ...row,
              position: "sticky",
              top: 44,
              zIndex: 15,
              background: `var(--statistics-ground, ${theme.ground})`,
              minHeight: 28,
              paddingTop: 6,
              paddingBottom: 12,
              marginBottom: 14,
              borderBottom: `1px solid ${theme.divider}`,
            }}
          >
            <button
              type="button"
              onClick={() => {
                setSelectedId(parent?.id ?? null);
                if (!parent) onClearSelection?.();
              }}
              style={{ ...plain, ...row, fontSize: 12, fontWeight: 600 }}
            >
              <ChevronLeft size={13} />
              <span style={clipped}>
                {parent?.title ?? t("All sessions", "全部会话")}
              </span>
            </button>
          </div>
          {selected ? (
            <InlineDetail
              session={selected}
              detail={detail}
              loading={loading}
              error={error}
              snapshot={snapshot}
              theme={theme}
              selectedEntry={selectedEntry}
              onEntry={setSelectedEntry}
              onSession={choose}
              loadMessages={() => void loadMessages()}
              loadingMessages={loadingMessages}
              reveal={() => {
                void actionRef
                  .current("open_session", { id: selected.id })
                  .catch((failure) => setError(String(failure)));
              }}
            />
          ) : (
            <Card style={card(theme)}>
              <span style={{ color: theme.secondary, fontSize: 12 }}>
                {error ?? t("Reading session usage…", "正在读取会话用量…")}
              </span>
            </Card>
          )}
          {error && (
            <Button variant="ghost" size="sm" onClick={retryDetail}>
              {t("Retry", "重试")}
            </Button>
          )}
        </>
      ) : (
        <>
          <div
            className="session-statistics-controls"
            style={{
              position: "sticky",
              top: 44,
              zIndex: 15,
              background: `var(--statistics-ground, ${theme.ground})`,
              display: "flex",
              flexDirection: "column",
              gap: 10,
              paddingTop: 6,
              paddingBottom: 12,
              marginBottom: 14,
              borderBottom: `1px solid ${theme.divider}`,
            }}
          >
            <div style={{ ...row, gap: 12, minHeight: 26 }}>
              <Segments
                small
                label={t("Session arrangement", "会话排列")}
                value={arrangement}
                onChange={(value) => setArrangement(value as Arrangement)}
                choices={[
                  { value: "day", label: t("Day", "按日期") },
                  { value: "active", label: t("Active", "活跃") },
                ]}
                theme={theme}
              />
              <span
                style={{
                  marginLeft: "auto",
                  color: theme.secondary,
                  fontSize: 11,
                  whiteSpace: "nowrap",
                  ...numeric,
                }}
              >
                {zh
                  ? `${arrangement === "active" ? "24 小时" : "7 天"}内 ${list.page?.total ?? 0} 个`
                  : `${list.page?.total ?? 0} in ${arrangement === "active" ? "24 hours" : "7 days"}`}
              </span>
              <SourceSelect
                label={t("Session source", "会话来源")}
                value={source}
                onChange={setSource}
                width={185}
                theme={theme}
              >
                <SelectItem value={ALL}>
                  {t("All sources", "全部来源")}
                </SelectItem>
                {sources.map((name) => (
                  <SelectItem key={name} value={name}>
                    {name}
                  </SelectItem>
                ))}
              </SourceSelect>
            </div>
            <div style={{ ...row, gap: 12, height: 26 }}>
              <SourceSelect
                label={t("Project path", "项目路径")}
                value={project}
                onChange={setProject}
                width={260}
                theme={theme}
              >
                <SelectItem value={ALL}>
                  {t("All projects", "全部项目")}
                </SelectItem>
                {projects.map((path) => (
                  <SelectItem key={path} value={path}>
                    {projectName(path, zh)}
                  </SelectItem>
                ))}
                {list.page?.projects.includes(null) && (
                  <SelectItem value="__unassigned__">
                    {t("Unassigned", "未归属项目")}
                  </SelectItem>
                )}
              </SourceSelect>
              <div
                style={{
                  ...row,
                  gap: 6,
                  height: 26,
                  flex: 1,
                  borderRadius: 6,
                  padding: "0 8px",
                  border: `1px solid ${theme.border}`,
                  background: theme.light ? "#fff" : "#1c1c1e",
                }}
              >
                <Search size={12} color={theme.secondary} />
                <Input
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  placeholder={t(
                    "Search sessions or paths",
                    "搜索会话标题或路径",
                  )}
                  aria-label={t("Search sessions", "搜索会话")}
                  style={{
                    border: 0,
                    background: "transparent",
                    boxShadow: "none",
                    height: 24,
                    padding: 0,
                    fontSize: 11,
                    color: theme.text,
                  }}
                />
                {query && (
                  <button
                    type="button"
                    aria-label={t("Clear search", "清空搜索")}
                    style={{ ...plain, color: theme.secondary }}
                    onClick={() => setQuery("")}
                  >
                    <X size={11} />
                  </button>
                )}
              </div>
            </div>
          </div>
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
                ? `已显示 ${filtered.length} / ${list.page!.total} 个会话。下方汇总仅统计已加载会话。`
                : `Showing ${filtered.length} of ${list.page!.total} sessions. Totals below cover loaded sessions.`}
            </div>
          )}
          {todaySessions.length > 0 && (
            <TodayCard
              sessions={todaySessions}
              theme={theme}
              partial={partial}
              onSelect={choose}
            />
          )}
          {arrangement === "active"
            ? filtered.length > 0 && (
                <>
                  <DayHeader
                    title={t("Last 24 hours", "最近 24 小时")}
                    count={filtered.length}
                    open
                    folded=""
                    theme={theme}
                    chevron={null}
                  />
                  {cards(filtered)}
                </>
              )
            : grouped.map((group) => {
                const open = (group.day === today) !== flipped.has(group.day);
                const groupTokens = group.sessions.reduce(
                  (sum, session) => sum + session.tokens,
                  0,
                );
                const groupMoney = sessionsMoney(group.sessions);
                return (
                  <div key={group.day}>
                    <DayHeader
                      title={titleForDay(group.day, today, zh)}
                      count={group.sessions.length}
                      open={open}
                      folded={[
                        groupMoney.value !== null
                          ? moneyText(groupMoney, zh)
                          : null,
                        groupTokens > 0 ? compact(groupTokens) : "—",
                      ]
                        .filter(Boolean)
                        .join(" · ")}
                      theme={theme}
                      onToggle={() =>
                        setFlipped((previous) => {
                          const next = new Set(previous);
                          if (next.has(group.day)) next.delete(group.day);
                          else next.add(group.day);
                          return next;
                        })
                      }
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
                {query || project !== ALL || source !== ALL
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
      )}
    </section>
  );
}
