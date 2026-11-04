import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import type { Session, SessionDetail } from "@agent-hud/protocol";
import { sessionWhere } from "../lib/sessions";
import {
  clipped,
  durationLabel,
  numeric,
  plain,
  row,
  sinceLabel,
  timeLabel,
  type Theme,
} from "./SessionTheme";

/** Where the session's first and latest prompts were typed, as Core tags them (`cli:iterm`, `mobile:iphone`). */
function OriginTags({ session, theme }: { session: Session; theme: Theme }) {
  const zh = Boolean(theme.zh);
  const tags = [
    {
      label: zh ? "首条消息" : "First prompt",
      help: zh
        ? "第一条消息在哪里打的：类别:来源"
        : "Where the first prompt was typed: category:source",
      tag: session.origins?.init,
    },
    {
      label: zh ? "最新消息" : "Latest prompt",
      help: zh
        ? "最新一条消息在哪里打的：类别:来源"
        : "Where the latest prompt was typed: category:source",
      tag: session.origins?.latest,
    },
  ].filter((item): item is typeof item & { tag: string } => Boolean(item.tag));
  if (tags.length === 0) return null;
  return (
    <div
      className="session-statistics-detail-origins"
      style={{
        ...row,
        gap: 12,
        fontSize: 11,
        color: theme.secondary,
        flexWrap: "wrap",
      }}
    >
      {tags.map((item) => (
        <span key={item.label} style={{ ...row, gap: 6 }} title={item.help}>
          {item.label}
          <span
            style={{
              color: theme.text,
              padding: "1px 7px",
              borderRadius: 999,
              border: `1px solid ${theme.border}`,
              userSelect: "text",
            }}
          >
            {item.tag}
          </span>
        </span>
      ))}
    </div>
  );
}

/** The session's name, a pill with how long ago its last message was, then model, project, client, start and duration, then where its prompts were typed. */
export function DetailHeader({
  session,
  detail,
  client,
  now,
  theme,
  reveal,
  revealDisabled,
  revealLabel,
}: {
  session: Session;
  detail: SessionDetail | null;
  client: string;
  now: number;
  theme: Theme;
  reveal: (() => void) | null;
  revealDisabled?: boolean;
  revealLabel: string;
}) {
  const zh = Boolean(theme.zh);
  const startedRaw =
    detail?.startedAt ??
    detail?.turns[0]?.timestamp ??
    session.startedAt ??
    null;
  const endedRaw =
    detail?.updatedAt ?? detail?.turns.at(-1)?.end ?? session.updatedAt ?? null;
  const started = startedRaw ? timeLabel(startedRaw, zh) : "—";
  const duration =
    startedRaw && endedRaw
      ? durationLabel(Date.parse(startedRaw), Date.parse(endedRaw), zh)
      : "—";
  return (
    <div
      className="session-statistics-detail-header"
      style={{ display: "grid", gap: 6, padding: "2px 2px 0" }}
    >
      <div style={{ ...row, alignItems: "flex-start", gap: 10 }}>
        <h2
          style={{
            fontSize: 18,
            fontWeight: 600,
            lineHeight: 1.3,
            padding: 0,
            margin: 0,
            userSelect: "text",
            flex: 1,
            minWidth: 0,
          }}
        >
          {session.title || (zh ? "未命名会话" : "Untitled session")}
        </h2>
        {(reveal || revealDisabled) && (
          <button
            type="button"
            disabled={revealDisabled}
            onClick={reveal ?? undefined}
            style={{
              ...plain,
              color: theme.action,
              fontSize: 11,
              flexShrink: 0,
              paddingTop: 4,
            }}
          >
            {revealLabel}
          </button>
        )}
      </div>
      <div
        style={{
          ...row,
          gap: 8,
          fontSize: 12,
          color: theme.secondary,
          flexWrap: "wrap",
        }}
      >
        <span
          style={{
            ...row,
            gap: 6,
            fontSize: 11,
            fontWeight: 600,
            color: theme.secondary,
            padding: "2px 8px",
            borderRadius: 999,
            background: `color-mix(in srgb, ${theme.secondary} 16%, transparent)`,
            ...numeric,
          }}
        >
          {sinceLabel(session, now, zh)}
        </span>
        <HUDLogo source={session.source} size={13} light={theme.light} />
        <span style={{ ...clipped, flex: 1 }} title={session.cwd ?? client}>
          {[
            ...sessionWhere(session, client),
            zh ? `开始于 ${started}` : `Started ${started}`,
            zh ? `时长 ${duration}` : `Duration ${duration}`,
          ].join(" · ")}
        </span>
      </div>
      <OriginTags session={session} theme={theme} />
    </div>
  );
}
