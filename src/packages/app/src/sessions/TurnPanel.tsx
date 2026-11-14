import { X } from "lucide-react";
import { Card } from "@agent-hud/ui/components/card";
import { countdown } from "@agent-hud/ui/format";
import type { SessionCall, SessionDetail } from "@agent-hud/protocol";
import { presentKinds } from "../lib/sessions";
import { SubagentTimeline } from "./SubagentTimeline";
import { TurnCallsTable } from "./TurnCallsTable";
import { turnNotes } from "./TurnHover";
import { CallsPlaceholder } from "./SessionPlaceholders";
import { Arrival, useSettled } from "./SessionMotion";
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
import { useFormat, useT } from "@agent-hud/ui/format-context";

/** A picked turn: when and how long, what Core sums of its calls, the message, and each call. In its own column the column's bar closes it, so it draws no × of its own. Calls read after it opened fade in over their grey shape's place. */
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
  close?: () => void;
}) {
  const t = useT();
  const { language } = useFormat();
  const settled = useSettled();
  const calls: SessionCall[] | undefined =
    entry.calls ??
    (detail.turnCalls?.turn === entry.id ? detail.turnCalls.calls : undefined);
  const summary =
    detail.turnCalls?.turn === entry.id ? detail.turnCalls.summary : undefined;
  const span =
    entry.end && Date.parse(entry.end) > Date.parse(entry.timestamp)
      ? countdown((Date.parse(entry.end) - Date.parse(entry.timestamp)) / 1000)
      : null;
  const notes = turnNotes(entry, language);
  const facts = [
    // The calls first, as Open's line starts.
    ...(entry.callCount != null ? notes.slice(0, 1) : []),
    span,
    summary && summary.subagentCalls > 0
      ? t("{{calls}} by {{count}} sub-agents", {
          calls: summary.subagentCalls,
          count: summary.subagentLogs,
        })
      : null,
    summary?.largestNew != null && summary.largestShare != null
      ? t("Largest call added {{tokens}}, {{share}}%", {
          tokens: compact(summary.largestNew),
          share: Math.round(summary.largestShare * 100),
        })
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
              ? `T${entry.index} · ${timeLabel(entry.timestamp, language)}`
              : t("Usage entry {{index}} · {{time}}", {
                  index: entry.index,
                  time: timeLabel(entry.timestamp, language),
                })}
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
              {t("Compacted")}
            </span>
          )}
        </div>
        <span style={{ ...row, gap: 12 }}>
          <strong style={{ fontSize: 13, ...numeric }}>
            {compact(entry.newTokens ?? entry.input + entry.output)}{" "}
            <span
              style={{ color: theme.secondary, fontWeight: 400, fontSize: 11 }}
            >
              {t("new")}
            </span>
          </strong>
          {close && (
            <button
              type="button"
              aria-label={t("Close this turn")}
              onClick={close}
              style={{ ...plain, color: theme.secondary }}
            >
              <X size={13} />
            </button>
          )}
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
                  {kindLabel(kind, language)}
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
        <Arrival late={settled} style={{ minWidth: 0 }}>
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
              entry.index ===
                Math.max(...detail.turns.map((turn) => turn.index))
            }
            theme={theme}
          />
        </Arrival>
      )}
      {byPrompt && calls === undefined && <CallsPlaceholder theme={theme} />}
      {calls && calls.length === 0 && (
        <div style={{ color: theme.secondary, fontSize: 11 }}>
          {t("The usage ledger holds no calls of this turn")}
        </div>
      )}
      {calls && calls.length > 0 && (
        <Arrival late={settled} style={{ minWidth: 0 }}>
          <TurnCallsTable calls={calls} theme={theme} />
        </Arrival>
      )}
    </Card>
  );
}
