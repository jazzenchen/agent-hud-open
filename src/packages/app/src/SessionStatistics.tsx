import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { Card } from "@agent-hud/ui/components/card";
import { Button } from "@agent-hud/ui/components/button";
import { systemTimeZone } from "@agent-hud/ui/format";
import { useSessionDetail, useSessions } from "./lib/use-sessions";
import type {
  NativeMethod,
  SessionsQuery,
  Snapshot,
} from "@agent-hud/protocol";
import { InlineDetail } from "./sessions/SessionDetailView";
import { NoSessionPicked, SessionBackBar } from "./sessions/SessionBackBar";
import { useWidth } from "./statistics/dashboard/Panel";
import { SessionList } from "./sessions/SessionList";
import { SessionListControls } from "./sessions/SessionListControls";
import { ALL, card, themeFor, type Arrangement } from "./sessions/SessionTheme";

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
  const now = Date.parse(snapshot.generatedAt);
  const timeZone = useMemo(systemTimeZone, []);
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
      timeZone,
    }),
    [arrangement, source, project, query, timeZone],
  );
  const revision =
    snapshot.sessionsRevision ?? (snapshot.mode === "demo" ? now : 0);
  const list = useSessions(listQuery, revision, active, action);
  const filtered = list.page?.items ?? [];
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
  // Wide, the list and the detail sit side by side; narrow, the detail
  // replaces the list until the way back is taken.
  const { ref, width } = useWidth<HTMLElement>();
  const split = width >= 1000;
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

  const listColumn = (
    <>
      <SessionListControls
        arrangement={arrangement}
        onArrangement={setArrangement}
        source={source}
        onSource={setSource}
        project={project}
        onProject={setProject}
        query={query}
        onQuery={setQuery}
        page={list.page}
        theme={theme}
        stickyTop={split ? 0 : 44}
      />
      <SessionList
        list={list}
        arrangement={arrangement}
        filtering={Boolean(query || project !== ALL || source !== ALL)}
        now={now}
        providers={snapshot.providers}
        active={active}
        flipped={flipped}
        onFlip={(day) =>
          setFlipped((previous) => {
            const next = new Set(previous);
            if (next.has(day)) next.delete(day);
            else next.add(day);
            return next;
          })
        }
        onSelect={choose}
        theme={theme}
        selectedId={split ? selectedId : null}
      />
    </>
  );
  // The detail, under a way back: to the parent session, or (in one
  // column) to the list.
  const detailColumn = selectedId ? (
    <>
      {(!split || parent) && (
        <SessionBackBar
          label={parent?.title ?? t("All sessions", "全部会话")}
          top={split ? 0 : 44}
          onBack={() => {
            setSelectedId(parent?.id ?? null);
            if (!parent) onClearSelection?.();
          }}
          theme={theme}
        />
      )}
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
    <NoSessionPicked theme={theme} />
  );

  return (
    <section
      ref={ref}
      className="session-statistics-native"
      style={
        split
          ? {
              ...contentStyle,
              padding: "0 0 0 22px",
              display: "grid",
              gridTemplateColumns: "clamp(440px, 42%, 580px) minmax(0, 1fr)",
              height: "calc(100vh - 44px)",
              overflow: "hidden",
            }
          : contentStyle
      }
      aria-label={t("Session statistics", "会话统计")}
    >
      {split ? (
        <>
          <div
            className="session-statistics-list-column"
            style={{
              overflowY: "auto",
              minHeight: 0,
              paddingRight: 16,
              paddingBottom: 24,
              borderRight: `1px solid ${theme.divider}`,
            }}
          >
            {listColumn}
          </div>
          <div
            className="session-statistics-detail-column"
            style={{
              overflowY: "auto",
              minHeight: 0,
              padding: "4px 22px 24px",
            }}
          >
            {detailColumn}
          </div>
        </>
      ) : selectedId ? (
        detailColumn
      ) : (
        listColumn
      )}
    </section>
  );
}
