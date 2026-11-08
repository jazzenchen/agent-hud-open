import { presentKinds } from "../lib/sessions";
import {
  card,
  ColorKey,
  compact,
  cost,
  integer,
  kindColor,
  kindLabel,
  numeric,
  row,
  timeLabel,
  type Entry,
  type Theme,
} from "./SessionTheme";

/** The facts Core gives a turn, as Open's chart notes them. */
export function turnNotes(entry: Entry, zh: boolean): string[] {
  const notes: string[] = [];
  if (entry.callCount != null)
    notes.push(
      zh
        ? `${entry.callCount} 次调用`
        : entry.callCount === 1
          ? "1 call"
          : `${integer(entry.callCount)} calls`,
    );
  if (entry.subagentNew)
    notes.push(
      (zh ? "子 agent 新增 " : "Sub-agents added ") +
        compact(entry.subagentNew),
    );
  if (entry.recached != null)
    notes.push(
      zh
        ? `缓存读取下降，未缓存输入 ${compact(entry.recached)}`
        : `Cache reads dropped, ${compact(entry.recached)} uncached input`,
    );
  if (entry.context != null)
    notes.push(
      (zh ? "上下文 " : "Context ") +
        compact(entry.context) +
        (entry.peakContext != null
          ? (zh ? " · 轮中最高 " : " · peak ") + compact(entry.peakContext)
          : ""),
    );
  const turnCost = entry.listCost ?? entry.pricedCost ?? null;
  if (turnCost != null)
    notes.push(
      cost(turnCost, zh, entry.listCost == null) +
        (zh ? " 按 API 价" : " at API prices"),
    );
  return notes;
}

/** What one turn holds, beside the bar the pointer is on. */
export function TurnHover({ entry, theme }: { entry: Entry; theme: Theme }) {
  const zh = Boolean(theme.zh);
  return (
    <div
      style={{
        ...card(theme, "9px 11px"),
        background: theme.surface,
        boxShadow: theme.light
          ? "0 4px 14px rgba(0,0,0,.12)"
          : "0 4px 14px rgba(0,0,0,.4)",
        display: "grid",
        gap: 3,
        fontSize: 11,
        width: 220,
      }}
    >
      <div
        style={{
          ...row,
          justifyContent: "space-between",
          color: theme.secondary,
        }}
      >
        <span>{`T${entry.index} · ${timeLabel(entry.timestamp, zh)}`}</span>
        {entry.compacted && (
          <span style={{ color: theme.compaction, fontWeight: 600 }}>
            {zh ? "已压缩" : "Compacted"}
          </span>
        )}
      </div>
      <strong style={{ fontSize: 13, ...numeric }}>
        {compact(entry.newTokens ?? entry.input + entry.output)}{" "}
        <span style={{ fontWeight: 400, color: theme.secondary, fontSize: 11 }}>
          {zh ? "新增 Token" : "new tokens"}
        </span>
      </strong>
      {presentKinds(entry.kinds).map(({ kind, value }) => (
        <span key={kind} style={{ ...row, gap: 6 }}>
          <ColorKey color={kindColor(theme, kind)}>
            <span style={{ color: theme.secondary }}>
              {kindLabel(kind, zh)}
            </span>
          </ColorKey>
          <span style={{ marginLeft: "auto", ...numeric }}>
            {compact(value)}
          </span>
        </span>
      ))}
      {turnNotes(entry, zh).map((note) => (
        <span key={note} style={{ color: theme.secondary }}>
          {note}
        </span>
      ))}
    </div>
  );
}
