import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { ChevronLeft } from "lucide-react";
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
import { SessionList } from "./sessions/SessionList";
import { SessionListControls } from "./sessions/SessionListControls";
import {
  ALL,
  card,
  clipped,
  plain,
  row,
  themeFor,
  type Arrangement,
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
          />
        </>
      )}
    </section>
  );
}
