import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { systemTimeZone } from "@agent-hud/ui/format";
import { useSessionDetail, useSessions } from "./lib/use-sessions";
import type {
  NativeMethod,
  SessionsQuery,
  Snapshot,
} from "@agent-hud/protocol";
import { SessionAside } from "./sessions/SessionAside";
import { NoSessionPicked, SessionBackBar } from "./sessions/SessionBackBar";
import { SessionColumns } from "./sessions/SessionColumns";
import { SessionDetailBody } from "./sessions/SessionDetailBody";
import { SessionList } from "./sessions/SessionList";
import { SessionListControls } from "./sessions/SessionListControls";
import { ALL, themeFor, type Arrangement } from "./sessions/SessionTheme";
import { sessionColumns, type Pick } from "./sessions/session-layout";
import { usePageWidth } from "./sessions/use-page-width";

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

/** The native Sessions page: the list, a picked session, and what was
 * picked in that session (a turn or a sub-agent), in as many columns as the
 * window holds. */
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
  const [pick, setPick] = useState<Pick | null>(null);
  const actionRef = useRef(action);
  actionRef.current = action;
  useEffect(() => {
    if (!initialSessionId) return;
    setSelectedId(initialSessionId);
    setPick(null);
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
  const turn = pick?.kind === "turn" ? pick.id : null;
  const state = useSessionDetail(selectedId, observed, active, action, turn);
  const { detail } = state;
  const selected = observed ?? detail?.session;
  const parent =
    detail?.parent !== undefined
      ? detail.parent
      : selected?.parentId
        ? (snapshot.sessions.find(
            (session) => session.id === selected.parentId,
          ) ?? filtered.find((session) => session.id === selected.parentId))
        : null;
  /** Shows a session (none: the list), with `next` open beside it. */
  const show = (id: string | null, next: Pick | null = null) => {
    setSelectedId(id);
    setPick(next);
    if (!id) onClearSelection?.();
  };
  // A step back closes the pick first, then goes to the parent or the list.
  const back = () => (pick ? setPick(null) : show(parent?.id ?? null));
  const backRef = useRef(back);
  backRef.current = back;
  useEffect(() => {
    if (!selectedId || !active) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey && event.key === "[") {
        event.preventDefault();
        backRef.current();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [selectedId, active]);
  const { ref, width } = usePageWidth<HTMLElement>(active);
  const {
    layout,
    shown,
    tracks,
    list: listRows,
  } = sessionColumns(width, Boolean(selectedId), Boolean(pick));
  const split = layout !== "one";
  // One column at a time scrolls the window: a pick opens at its top.
  useEffect(() => {
    if (active && !split && pick) document.scrollingElement?.scrollTo(0, 0);
  }, [split, pick?.kind, pick?.id]);
  const open = (id: string) => actionRef.current("open_session", { id });

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
        onSelect={(id) => show(id)}
        theme={theme}
        selectedId={split ? selectedId : null}
        narrow={listRows > 0 && listRows < 500}
      />
    </>
  );
  // The session, under a way back: to the parent session, or (where the
  // list is not beside it) to the list, which a pick beside it hides.
  const listHidden = !shown.includes("list");
  const sessionColumn = selectedId ? (
    <>
      {(listHidden || parent) && (
        <SessionBackBar
          label={parent?.title ?? t("All sessions", "全部会话")}
          top={split ? 0 : 44}
          onBack={() =>
            parent ? show(parent.id) : split ? setPick(null) : show(null)
          }
          theme={theme}
        />
      )}
      <SessionDetailBody
        session={selected}
        state={state}
        snapshot={snapshot}
        theme={theme}
        open={open}
        selectedEntry={turn}
        onEntry={(id) => setPick(id ? { kind: "turn", id } : null)}
        onSession={(id) => setPick({ kind: "session", id })}
        pickedSession={pick?.kind === "session" ? pick.id : null}
        turnBeside
      />
    </>
  ) : (
    <NoSessionPicked theme={theme} />
  );
  const pickColumn =
    pick && selectedId ? (
      <SessionAside
        pick={pick}
        session={selected}
        detail={detail}
        layout={layout}
        snapshot={snapshot}
        theme={theme}
        active={active}
        action={action}
        onClose={() => setPick(null)}
        onShift={(id) => show(pick.id, { kind: "session", id })}
      />
    ) : null;

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
    <SessionColumns
      sectionRef={ref}
      label={t("Session statistics", "会话统计")}
      style={contentStyle}
      split={split}
      shown={shown}
      tracks={tracks}
      pickKey={pick ? `${pick.kind}:${pick.id}` : ""}
      theme={theme}
      columns={{ list: listColumn, session: sessionColumn, pick: pickColumn }}
    />
  );
}
