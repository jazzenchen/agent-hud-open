import type { CSSProperties } from "react";
import type {
  PanelLayout,
  SessionsResponse,
  Snapshot,
} from "@agent-hud/protocol";
import { tokensShort } from "@agent-hud/ui/format";
import { sourceID, sourcesForMark } from "../model";
import { usePanelQuery, usePanelServices } from "../PanelContext";
import { noButtonChrome, palette, tabular, type Logo } from "./theme";
import { useGround } from "../ground";
import { useT } from "@agent-hud/ui/format-context";
import { useListedSessions } from "./listed-sessions";
import { noSessionsMatch } from "./session-filters";

/** One line of text that shortens with an ellipsis where it does not fit. */
const clipped: CSSProperties = {
  minWidth: 0,
  whiteSpace: "nowrap",
  overflow: "hidden",
  textOverflow: "ellipsis",
};

export function SourceSessions({
  snapshot,
  focusVendor,
  light,
  Logo,
  onStats,
  layout = {},
  active = true,
  bare = false,
}: {
  snapshot: Snapshot;
  focusVendor?: string;
  light: boolean;
  Logo: Logo;
  onStats?: () => void;
  layout?: PanelLayout["sessions"];
  /** No line above it: it heads a column of its own. */
  bare?: boolean;
  active?: boolean;
}) {
  const t = useT();
  const services = usePanelServices();
  const listed = useListedSessions();
  // The Sessions page: the list, or one session's usage.
  const openSessions = (sessionId?: string) =>
    listed ? listed.openStats({ tab: "sessions", sessionId }) : onStats?.();
  const limit = layout.limit ?? 5;
  const selectedSources = focusVendor
    ? sourcesForMark(snapshot, focusVendor)
    : layout.agents?.length
      ? layout.agents.map((source) => sourceID(snapshot, source))
      : undefined;
  const week = layout.period === "week";
  const query = usePanelQuery<SessionsResponse>(
    "sessions",
    {
      scope: week ? "recent" : "active",
      limit,
      ...(selectedSources ? { sources: selectedSources } : {}),
      ...(layout.projects?.length ? { projects: layout.projects } : {}),
    },
    snapshot.sessionsRevision,
    active,
  );
  const colors = palette(useGround(light));
  // Snapshot rows provide a first paint only; the complete-index query owns
  // ordering and totals. Both list the newest last message first and look
  // at no session state (owner, 2026-10-26).
  const fallback = snapshot.sessions
    .filter(
      (session) =>
        (!selectedSources ||
          selectedSources.includes(sourceID(snapshot, session.source))) &&
        (!layout.projects?.length ||
          layout.projects.includes(
            session.project ?? session.cwd ?? "unassigned",
          )),
    )
    .sort(
      (a, b) =>
        Date.parse(b.lastMessageAt ?? b.updatedAt) -
        Date.parse(a.lastMessageAt ?? a.updatedAt),
    );
  const sessions = query.data?.items ?? fallback.slice(0, limit);
  const total = query.data?.total ?? fallback.length;
  // A filter of the layout that keeps none of the sessions there are says
  // so, and the header then names no window it leaves them out of.
  const own = focusVendor
    ? snapshot.sessions.filter((session) =>
        selectedSources!.includes(sourceID(snapshot, session.source)),
      ).length
    : snapshot.sessions.length;
  const noneMatch = noSessionsMatch(
    layout,
    Boolean(focusVendor),
    sessions.length,
    own,
  );
  const status = week ? t("Last 7 days") : t("Last 24 hours");
  return (
    <section
      className="hud-source-sessions"
      style={{
        display: "grid",
        // As wide as its place, never wider: in a row of sections its titles
        // shorten instead of running into the next one.
        gridTemplateColumns: "minmax(0, 1fr)",
        gap: "var(--hud-session-gap, 6px)",
        paddingTop: bare ? 0 : "var(--hud-divided-gap, 8px)",
        boxShadow: bare ? undefined : `inset 0 1px 0 ${colors.divider}`,
        fontSize: 12,
      }}
    >
      <button
        title={t("Show sessions")}
        onClick={() => openSessions()}
        disabled={!onStats && !listed}
        style={{
          ...noButtonChrome,
          display: "flex",
          alignItems: "baseline",
          gap: 8,
          width: "100%",
          fontSize: 13,
          lineHeight: "16px",
        }}
      >
        <strong style={{ fontWeight: 600, color: colors.text, ...clipped }}>
          {t("Sessions")}
        </strong>
        <span
          style={{
            marginLeft: "auto",
            ...clipped,
            // The window shortens before the heading does.
            flexShrink: 1000,
            ...tabular,
            fontSize: 12,
            color: colors.secondary,
            // Without a status the line keeps its room.
            visibility: noneMatch ? "hidden" : undefined,
          }}
        >
          {status}
        </span>
      </button>
      {sessions.slice(0, limit).map((session) => (
        // The whole row opens the session's page, as its heading opens the list.
        <button
          key={session.id}
          disabled={!onStats && !listed}
          title={`${session.title}\n${t("Show session details")}`}
          onClick={() =>
            listed
              ? openSessions(session.id)
              : services?.selectSession
                ? services.selectSession(session)
                : onStats?.()
          }
          style={{
            ...noButtonChrome,
            display: "flex",
            alignItems: "center",
            gap: 8,
            width: "100%",
            color: colors.secondary,
            lineHeight: "15px",
            minHeight: 15,
          }}
        >
          <span
            style={{
              display: "block",
              width: 14,
              height: 14,
              flex: "0 0 14px",
            }}
          >
            <Logo source={session.source} size={14} light={light} />
          </span>
          <RowTitle
            parts={sessionTitleParts(session.title, session.cwd)}
            untitled={t("(untitled session)")}
          />
          <span
            style={{
              ...tabular,
              fontSize: 12,
              whiteSpace: "nowrap",
              flexShrink: 0,
            }}
          >
            {tokensShort(session.tokens)} tok
          </span>
        </button>
      ))}
      {!sessions.length && (
        <span style={{ color: colors.secondary, lineHeight: "15px" }}>
          {noneMatch ? t("No sessions match") : t("No sessions yet")}
        </span>
      )}
      {total > limit && (
        <button
          style={{ ...noButtonChrome, color: colors.secondary, fontSize: 12 }}
          disabled={!onStats && !listed}
          onClick={() => openSessions()}
        >
          {t("+{{number}} more", { number: total - limit })}
        </button>
      )}
    </section>
  );
}

/** A row's title and folder, shortening where the row is narrow so its tokens
 * stay whole: the folder first, as it takes only the room the title leaves,
 * while a few letters of it fit, and below that it leaves (a first item as tall
 * as the line holds it, so the folder wraps onto a hidden second line instead
 * of a stub); then the title. */
function RowTitle({
  parts: [task, folder],
  untitled,
}: {
  parts: [string, string | null];
  untitled: string;
}) {
  return (
    <span style={{ display: "flex", flex: 1, minWidth: 0 }}>
      <span style={clipped}>{task || untitled}</span>
      {folder && (
        <span
          style={{
            display: "flex",
            flexWrap: "wrap",
            flex: "1 1 0",
            height: 15,
            minWidth: 0,
            overflow: "hidden",
          }}
        >
          <span style={{ height: 15 }} />
          <span
            style={{
              ...clipped,
              flex: "1 1 40px",
              minWidth: 40,
              maxWidth: "max-content",
              whiteSpace: "pre",
            }}
          >
            {` · ${folder}`}
          </span>
        </span>
      )}
    </span>
  );
}

/** The row's title (Open's HoverPanelView.sessionTitle): the task cut short,
 * then the session's folder, so rows of one task in different folders differ. */
export function sessionTitleParts(
  task: string,
  cwd: string | null,
): [string, string | null] {
  const terminal = cwd?.split(/[\\/]/).filter(Boolean).pop();
  if (!terminal || !task) return [task, null];
  const words = task.split(" ").filter(Boolean);
  const short =
    words.length > 3
      ? words.slice(0, 3).join(" ")
      : [...task].slice(0, 14).join("");
  return [short, terminal];
}
