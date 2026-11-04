import { X } from "lucide-react";
import { Card } from "@agent-hud/ui/components/card";
import { countdown } from "@agent-hud/ui/format";
import type { SessionCall, SessionDetail } from "@agent-hud/protocol";
import { presentKinds } from "../lib/sessions";
import { SubagentTimeline } from "./SubagentTimeline";
import { TurnCallsTable } from "./TurnCallsTable";
import { turnNotes } from "./TurnHover";
import {
  card,
  ColorKey,
  compact,
  kindColor,
  kindLabel,
  numeric,
  plain,
  row,
  timeLabel,
  type Entry,
  type Theme,
} from "./SessionTheme";

/** A picked turn: when and how long, what Core sums of its calls, the message, and each call. */
export function TurnPanel({
  entry,
  detail,
  byPrompt,
  theme,
  close,
}: {
  entry: Entry;
  detail: SessionDetail;
  byPrompt: boolean;
  theme: Theme;
  close: () => void;
}) {
  const zh = Boolean(theme.zh);
  const calls: SessionCall[] | undefined =
    entry.calls ??
    (detail.turnCalls?.turn === entry.id ? detail.turnCalls.calls : undefined);
  const summary =
    detail.turnCalls?.turn === entry.id ? detail.turnCalls.summary : undefined;
  const span =
    entry.end && Date.parse(entry.end) > Date.parse(entry.timestamp)
      ? countdown((Date.parse(entry.end) - Date.parse(entry.timestamp)) / 1000)
      : null;
  const notes = turnNotes(entry, zh);
  const facts = [
    // The calls first, as Open's line starts.
    ...(entry.callCount != null ? notes.slice(0, 1) : []),
    span,
    summary && summary.subagentCalls > 0
      ? zh
        ? `其中子 agent ${summary.subagentCalls} 次（${summary.subagentLogs} 个）`
        : `${summary.subagentCalls} by ${summary.subagentLogs === 1 ? "a sub-agent" : `${summary.subagentLogs} sub-agents`}`
      : null,
    summary?.largestNew != null && summary.largestShare != null
      ? zh
        ? `新增最多的一次 ${compact(summary.largestNew)}，占 ${Math.round(summary.largestShare * 100)}%`
        : `Largest call added ${compact(summary.largestNew)}, ${Math.round(summary.largestShare * 100)}%`
      : null,
    ...(entry.callCount != null ? notes.slice(1) : notes),
  ].filter(Boolean);
  return (
    <Card
      style={{ ...card(theme), animation: "session-fade-in 200ms ease" }}
      className="session-turn-panel"
    >
      <div style={{ ...row, justifyContent: "space-between" }}>
        <div style={{ ...row, gap: 8 }}>
          <strong style={{ fontSize: 13, fontWeight: 600 }}>
            {byPrompt
              ? `T${entry.index} · ${timeLabel(entry.timestamp, zh)}`
              : (zh
                  ? `用量记录 ${entry.index}`
                  : `Usage entry ${entry.index}`) +
                ` · ${timeLabel(entry.timestamp, zh)}`}
          </strong>
          {entry.compacted && (
            <span
              style={{
                fontSize: 10,
                fontWeight: 600,
                color: theme.compaction,
                background: `color-mix(in srgb, ${theme.compaction} 14%, transparent)`,
                padding: "1px 6px",
                borderRadius: 4,
              }}
            >
              {zh ? "上下文压缩" : "Compacted"}
            </span>
          )}
        </div>
        <span style={{ ...row, gap: 12 }}>
          <strong style={{ fontSize: 13, ...numeric }}>
            {compact(entry.newTokens ?? entry.input + entry.output)}{" "}
            <span
              style={{ color: theme.secondary, fontWeight: 400, fontSize: 11 }}
            >
              {zh ? "新增" : "new"}
            </span>
          </strong>
          <button
            type="button"
            aria-label={zh ? "收起这一轮" : "Close this turn"}
            onClick={close}
            style={{ ...plain, color: theme.secondary }}
          >
            <X size={13} />
          </button>
        </span>
      </div>
      <div style={{ color: theme.secondary, fontSize: 11 }}>
        {facts.join(" · ")}
      </div>
      {entry.kinds && (
        <div
          style={{
            ...row,
            flexWrap: "wrap",
            columnGap: 16,
            rowGap: 4,
            fontSize: 11,
          }}
        >
          {presentKinds(entry.kinds).map(({ kind, value }) => (
            <span key={kind} style={{ ...row, gap: 6 }}>
              <ColorKey color={kindColor(theme, kind)}>
                <span style={{ color: theme.secondary }}>
                  {kindLabel(kind, zh)}
                </span>
              </ColorKey>
              <span style={numeric}>{compact(value)}</span>
            </span>
          ))}
        </div>
      )}
      {entry.message && (
        <div
          style={{
            fontSize: 12,
            color: theme.secondary,
            whiteSpace: "pre-wrap",
            userSelect: "text",
          }}
        >
          {entry.message}
        </div>
      )}
      {calls?.some((call) => call.subagent) && (
        <SubagentTimeline
          calls={calls}
          agents={
            detail.turnCalls?.turn === entry.id
              ? (detail.turnCalls.agents ?? [])
              : []
          }
          start={Date.parse(entry.timestamp)}
          end={Date.parse(entry.end ?? entry.timestamp)}
          live={
            detail.session.state === "running" &&
            entry.index === Math.max(...detail.turns.map((turn) => turn.index))
          }
          theme={theme}
        />
      )}
      {byPrompt && calls === undefined && (
        <div style={{ color: theme.secondary, fontSize: 11 }}>
          {zh ? "正在读取这一轮的调用…" : "Reading this turn's calls…"}
        </div>
      )}
      {calls && calls.length === 0 && (
        <div style={{ color: theme.secondary, fontSize: 11 }}>
          {zh
            ? "用量库里没有这一轮的调用"
            : "The usage ledger holds no calls of this turn"}
        </div>
      )}
      {calls && calls.length > 0 && (
        <TurnCallsTable calls={calls} theme={theme} />
      )}
    </Card>
  );
}
