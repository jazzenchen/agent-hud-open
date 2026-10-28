import { useState, type ReactNode } from "react";
import { ChevronRight, Layers, RotateCw, X } from "lucide-react";
import { Card } from "@agent-hud/ui/components/card";
import { Button } from "@agent-hud/ui/components/button";
import { Tabs, TabsList, TabsTrigger } from "@agent-hud/ui/components/tabs";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import {
  sessionClient,
  sessionWhere,
  tokenKindOrder,
  type TokenKindName,
} from "../lib/sessions";
import {
  countdown,
  dateFormat,
  localeDateTime,
  moneyAmount,
  share,
} from "@agent-hud/ui/format";
import type {
  Session,
  SessionCall,
  SessionDetail,
  Snapshot,
} from "../lib/types";
import { SubagentTimeline } from "./SubagentTimeline";
import {
  ContextChart,
  ShareRows,
  TurnBarsChart,
  type Measure,
} from "./SessionCharts";
import {
  card,
  clipped,
  ColorKey,
  compact,
  cost,
  durationLabel,
  integer,
  kindColor,
  kindNames,
  moneyText,
  numeric,
  plain,
  row,
  sessionMoney,
  sinceLabel,
  timeLabel,
  type Entry,
  type Theme,
  unpricedHelp,
} from "./SessionTheme";

function formatMessageText(value: unknown): string {
  if (typeof value === "string") return value;
  if (value == null) return "";
  if (Array.isArray(value)) {
    return value
      .map((item) => {
        if (typeof item === "string") return item;
        if (item && typeof item === "object") {
          const record = item as Record<string, unknown>;
          if (typeof record.text === "string") return record.text;
          if (record.content !== undefined)
            return formatMessageText(record.content);
          if (record.type === "image") return "[image]";
          if (typeof record.tool_name === "string")
            return `[tool: ${record.tool_name}]`;
        }
        try {
          return JSON.stringify(item);
        } catch {
          return String(item);
        }
      })
      .filter(Boolean)
      .join("\n");
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

/** An MCP tool by its own name, without its server's (Open's SessionCharts.toolName). */
function toolName(name: string): string {
  return name.startsWith("mcp__") ? (name.split("__").at(-1) ?? name) : name;
}

/** The facts Core gives a turn, as Open's chart notes them. */
function turnNotes(entry: Entry, zh: boolean): string[] {
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
        ? `缓存过期，重发 ${compact(entry.recached)}`
        : `Cache expired, ${compact(entry.recached)} sent again`,
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

/** The kinds present in `tokens`, in Open's order (`TokenKind.allCases`). */
function presentKinds(
  tokens: Partial<Record<TokenKindName, number>> | null | undefined,
): { kind: TokenKindName; value: number }[] {
  return tokenKindOrder
    .map((kind) => ({ kind, value: tokens?.[kind] ?? 0 }))
    .filter((entry) => entry.value > 0);
}

/** A card's title line: its name, and on the right what it measures or a control. */
function CardTitle({
  title,
  right,
  theme,
}: {
  title: ReactNode;
  right?: ReactNode;
  theme: Theme;
}) {
  return (
    <div style={{ ...row, justifyContent: "space-between", minHeight: 22 }}>
      <strong style={{ fontSize: 13, fontWeight: 600 }}>{title}</strong>
      {right != null && (
        <span style={{ color: theme.secondary, fontSize: 11, ...row, gap: 8 }}>
          {right}
        </span>
      )}
    </div>
  );
}

/** What one turn holds, beside the bar the pointer is on. */
function TurnHover({ entry, theme }: { entry: Entry; theme: Theme }) {
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
              {kindNames[kind][zh ? 1 : 0]}
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

export function DetailFigures({
  session,
  detail,
  theme,
}: {
  session: Session;
  detail: SessionDetail | null;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const context =
    detail?.contextTokens ?? detail?.turns.at(-1)?.context ?? null;
  const percent = (part: number) => `${Math.round(part * 100)}%`;
  // The session with its sub-agents, as Core counts it on its row too.
  const totals = detail?.usage ?? session.usage ?? null;
  const language = zh ? "zh" : "en";
  // A priced account's own estimate comes first, as Open's SessionMoney.
  const account = session.accountCost !== undefined;
  const money = sessionMoney(session);
  const unpricedModels = (detail?.models ?? [])
    .filter((model) => model.priced === false)
    .map((model) => model.name ?? model.id ?? "")
    .filter(Boolean);
  const credits = detail?.credits ?? null;
  const breakdown = detail?.turnsBy != null;
  const calls = detail?.callCount ?? 0;
  const items = [
    {
      title: zh ? "Token" : "Tokens",
      value: compact(totals?.total ?? session.tokens),
      note: detail?.subagents
        ? (zh ? "含子 agent " : "Sub-agents ") + compact(detail.subagents.total)
        : totals
          ? zh
            ? `不含缓存读取 ${compact(totals.total - totals.cacheRead)}`
            : `Excl. cache reads ${compact(totals.total - totals.cacheRead)}`
          : zh
            ? "不含缓存读取 —"
            : "Excl. cache reads —",
      help: zh
        ? "五类合计，含缓存读取"
        : "All five kinds, cache reads included",
    },
    account
      ? {
          title: zh ? "费用" : "Cost",
          value: session.accountCost
            ? `≈${moneyAmount(session.accountCost.amount, session.accountCost.currency, language, true)}`
            : "—",
          note: zh ? "本会话" : "This session",
          help: zh
            ? "按账户价格估算的本会话费用"
            : "This session's cost at the account's prices",
        }
      : {
          title: zh ? "费用" : "Cost",
          value: moneyText(money, zh),
          note:
            money.value === null
              ? "—"
              : money.unpriced > 0
                ? zh
                  ? "部分 · 按 API 价"
                  : "Partial · at API prices"
                : zh
                  ? "按 API 价"
                  : "At API prices",
          help: [
            zh
              ? "同样的调用按厂商 API 公开价计算的费用，不是实际扣费"
              : "What the same calls cost at the vendor's API list price; not a charge",
            unpricedHelp(money.unpriced, zh, unpricedModels),
          ]
            .filter(Boolean)
            .join(zh ? "。" : ". "),
        },
    ...(credits
      ? [
          {
            title: zh ? "积分" : "Credits",
            value: credits.text,
            note: credits.unattributedText
              ? zh
                ? `未归因 ${credits.unattributedText}`
                : `Unattributed ${credits.unattributedText}`
              : zh
                ? "已按模型归因"
                : "Attributed to models",
            help: zh
              ? "WorkBuddy 自身计量记录的会话积分消耗；无法唯一确定模型的积分单独列为未归因"
              : "Credits WorkBuddy's own metering charged the session; the part no one model could be pinned on shows as unattributed",
          },
          ...(credits.used != null && credits.size != null
            ? [
                {
                  title: zh ? "计量上下文" : "Metered context",
                  value: compact(credits.used),
                  note: (zh ? "共 " : "Of ") + compact(credits.size),
                  help: zh
                    ? "WorkBuddy 自身计量记录的上下文用量与上限"
                    : "The context usage and limit WorkBuddy's own metering recorded",
                },
              ]
            : []),
        ]
      : []),
    breakdown
      ? {
          title: zh ? "轮次" : "Turns",
          value: detail?.turnCount ? integer(detail.turnCount) : "—",
          note: zh
            ? `${integer(calls)} 次调用`
            : calls === 1
              ? "1 call"
              : `${integer(calls)} calls`,
          help: zh ? "每个 prompt 开始一轮" : "Each prompt starts a turn",
        }
      : {
          title: zh ? "用量记录" : "Usage entries",
          value: detail ? integer(detail.turns.length) : "—",
          note: detail
            ? zh
              ? `已加载 ${detail.calls.length} 次调用`
              : `${detail.calls.length} calls loaded`
            : "—",
          help: zh
            ? "已记录的用量变动条目。"
            : "Recorded usage changes; these are not assumed to be prompt-defined turns.",
        },
    {
      title: zh ? "上下文" : "Context",
      value:
        detail?.contextFill != null
          ? percent(detail.contextFill)
          : context !== null
            ? compact(context)
            : session.contextPercent !== null
              ? `${Math.round(session.contextPercent)}%`
              : "—",
      note:
        detail?.contextWindow != null && detail.contextWindow > 0
          ? zh
            ? `共 ${compact(detail.contextWindow)}`
            : `Of ${compact(detail.contextWindow)}`
          : zh
            ? "最近一次调用"
            : "Latest call",
      help: zh
        ? "最近一次模型调用的输入：新输入、缓存写入与缓存读取"
        : "The latest model call's input: fresh, cache writes and cache reads",
      warn: detail?.contextFill != null && detail.contextFill >= 0.8,
    },
    {
      title: zh ? "缓存命中" : "Cache hits",
      value: detail?.cacheHitRate != null ? percent(detail.cacheHitRate) : "—",
      note: detail?.usage
        ? zh
          ? `${compact(detail.usage.cacheRead)} 读取 · ${compact(detail.usage.cacheWrite)} 写入`
          : `${compact(detail.usage.cacheRead)} read · ${compact(detail.usage.cacheWrite)} write`
        : zh
          ? "提示读自缓存"
          : "Of prompts, from cache",
      help: zh
        ? "缓存读取占全部提示的比例：新输入、缓存写入与缓存读取"
        : "Cache reads as a share of every prompt: fresh input, cache writes and cache reads",
    },
  ];
  return (
    <Card
      style={{
        ...card(theme, "10px 8px"),
        display: "grid",
        gridTemplateColumns: `repeat(${items.length},minmax(0,1fr))`,
        gap: 0,
      }}
    >
      {items.map((item, index) => (
        <div
          key={item.title}
          title={item.help}
          style={{
            padding: "0 7px",
            minWidth: 0,
            textAlign: "center",
            borderLeft: index > 0 ? `1px solid ${theme.divider}` : 0,
          }}
        >
          <div style={{ fontSize: 11, color: theme.secondary, ...clipped }}>
            {item.title}
          </div>
          <div
            style={{
              fontSize: 17,
              fontWeight: 600,
              margin: "4px 0",
              color: "warn" in item && item.warn ? theme.warning : undefined,
              ...numeric,
            }}
          >
            {item.value}
          </div>
          <div style={{ fontSize: 11, color: theme.secondary, ...clipped }}>
            {item.note}
          </div>
        </div>
      ))}
    </Card>
  );
}

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
function DetailHeader({
  session,
  detail,
  client,
  now,
  theme,
  reveal,
  revealLabel,
}: {
  session: Session;
  detail: SessionDetail | null;
  client: string;
  now: number;
  theme: Theme;
  reveal: (() => void) | null;
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
        {reveal && (
          <button
            type="button"
            onClick={reveal}
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

/** The marks the turn chart uses, named once above it. */
function ChartLegend({
  entries,
  measure,
  theme,
}: {
  entries: Entry[];
  measure: Measure;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const kinds = (
    ["cacheWrite", "input", "reasoning", "output"] as const
  ).filter((kind) => entries.some((entry) => (entry.kinds?.[kind] ?? 0) > 0));
  const helped = entries.some((entry) => (entry.subagentNew ?? 0) > 0);
  const resent = entries.some((entry) => entry.recached != null);
  const compacted = entries.some((entry) => entry.compacted);
  return (
    <div
      style={{
        ...row,
        flexWrap: "wrap",
        columnGap: 14,
        rowGap: 4,
        color: theme.secondary,
        fontSize: 11,
      }}
    >
      {measure === "cost" ? (
        <ColorKey color={theme.accent}>
          {zh ? "按 API 价，含缓存读取" : "API prices, cache reads included"}
        </ColorKey>
      ) : (
        kinds.map((kind) => (
          <ColorKey key={kind} color={kindColor(theme, kind)}>
            {kindNames[kind][zh ? 1 : 0]}
          </ColorKey>
        ))
      )}
      {helped && (
        <span style={{ ...row, gap: 4 }}>
          <Layers size={10} aria-hidden="true" />
          {measure === "cost"
            ? zh
              ? "子 agent 参与"
              : "Sub-agents helped"
            : zh
              ? "子 agent 参与，淡色为其部分"
              : "Sub-agents helped, their part faded"}
        </span>
      )}
      {resent && (
        <span style={{ ...row, gap: 4 }}>
          <RotateCw
            size={10}
            color={theme.warning}
            strokeWidth={2.6}
            aria-hidden="true"
          />
          {zh ? "缓存过期，上下文重发" : "Cache expired, context sent again"}
        </span>
      )}
      {compacted && (
        <span style={{ ...row, gap: 5, color: theme.compaction }}>
          <span
            aria-hidden="true"
            style={{
              display: "inline-block",
              height: 10,
              borderLeft: `1.5px dashed ${theme.compaction}`,
            }}
          />
          {zh ? "上下文压缩" : "Compacted"}
        </span>
      )}
    </div>
  );
}

/** A picked turn: when and how long, what Core sums of its calls, the message, and each call. */
function TurnPanel({
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
  const reasoning = calls?.some((call) => (call.reasoning ?? 0) > 0);
  const contexts = calls?.some((call) => call.context != null);
  const tools = calls?.some((call) => (call.tools?.length ?? 0) > 0);
  const cell = { padding: "5px 6px", textAlign: "right" as const };
  const head = {
    padding: "4px 6px",
    fontWeight: 500,
    textAlign: "right" as const,
  };
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
                  {kindNames[kind][zh ? 1 : 0]}
                </span>
              </ColorKey>
              <span style={numeric}>{compact(value)}</span>
            </span>
          ))}
        </div>
      )}
      {entry.message != null && formatMessageText(entry.message) !== "" && (
        <div
          style={{
            fontSize: 12,
            color: theme.secondary,
            whiteSpace: "pre-wrap",
            userSelect: "text",
          }}
        >
          {formatMessageText(entry.message)}
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
        <div
          style={{
            overflowX: "auto",
            borderTop: `1px solid ${theme.divider}`,
            paddingTop: 4,
          }}
        >
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontSize: 11,
              ...numeric,
            }}
          >
            <thead>
              <tr
                style={{
                  color: theme.secondary,
                  borderBottom: `1px solid ${theme.divider}`,
                }}
              >
                <th style={{ ...head, textAlign: "left", paddingLeft: 0 }}>
                  {zh ? "时间" : "Time"}
                </th>
                <th style={{ ...head, textAlign: "left" }}>
                  {zh ? "模型" : "Model"}
                </th>
                <th style={head}>{zh ? "新增" : "New"}</th>
                <th style={head}>{zh ? "缓存写" : "Cache W"}</th>
                <th style={head}>{zh ? "输入" : "Input"}</th>
                {reasoning && <th style={head}>{zh ? "思考" : "Reasoning"}</th>}
                <th style={head}>{zh ? "输出" : "Output"}</th>
                <th style={head}>{zh ? "缓存读" : "Cache R"}</th>
                {contexts && <th style={head}>{zh ? "上下文" : "Context"}</th>}
                {tools && (
                  <th style={{ ...head, textAlign: "left", paddingRight: 0 }}>
                    {zh ? "发起工具" : "Asked for"}
                  </th>
                )}
              </tr>
            </thead>
            <tbody>
              {calls.map((call, index) => (
                <tr
                  key={call.id ?? index}
                  style={{
                    borderTop: index > 0 ? `1px solid ${theme.divider}` : 0,
                  }}
                >
                  <td
                    style={{
                      ...cell,
                      textAlign: "left",
                      paddingLeft: 0,
                      color: theme.secondary,
                    }}
                  >
                    {dateFormat(zh ? "zh-CN" : "en-GB", {
                      hour: "numeric",
                      minute: "numeric",
                      second: "numeric",
                      hour12: false,
                    }).format(new Date(call.timestamp))}
                  </td>
                  <td style={{ ...cell, textAlign: "left", maxWidth: 160 }}>
                    <span style={{ ...row, gap: 5 }}>
                      <span style={clipped}>
                        {call.modelName || call.model || "—"}
                      </span>
                      {call.subagent && (
                        <span
                          style={{
                            fontSize: 10,
                            fontWeight: 600,
                            padding: "0 5px",
                            borderRadius: 4,
                            background: theme.segment,
                            color: theme.secondary,
                            flexShrink: 0,
                          }}
                        >
                          {zh ? "子 agent" : "Sub-agent"}
                        </span>
                      )}
                      {call.recached != null && (
                        <RotateCw
                          size={10}
                          color={theme.warning}
                          strokeWidth={2.6}
                          aria-label={zh ? "缓存过期" : "Cache expired"}
                        />
                      )}
                    </span>
                  </td>
                  <td style={{ ...cell, fontWeight: 600 }}>
                    {call.newTokens != null ? compact(call.newTokens) : "—"}
                  </td>
                  <td style={cell}>{compact(call.cacheWrite ?? 0)}</td>
                  <td style={cell}>{compact(call.input)}</td>
                  {reasoning && (
                    <td style={cell}>{compact(call.reasoning ?? 0)}</td>
                  )}
                  <td style={cell}>{compact(call.output)}</td>
                  <td style={{ ...cell, color: theme.secondary }}>
                    {compact(call.cacheRead ?? 0)}
                  </td>
                  {contexts && (
                    <td style={{ ...cell, color: theme.secondary }}>
                      {call.context == null ? "—" : compact(call.context)}
                    </td>
                  )}
                  {tools && (
                    <td
                      title={(call.tools ?? []).join(", ")}
                      style={{
                        ...cell,
                        textAlign: "left",
                        paddingRight: 0,
                        color: theme.secondary,
                        maxWidth: 170,
                        ...clipped,
                      }}
                    >
                      {(call.tools ?? [])
                        .map(toolName)
                        .join(zh ? "、" : ", ") || "—"}
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Card>
  );
}

/** What the tokens were: a bar and each kind present with its share, the sub-agents' part noted. */
function KindsCard({ detail, theme }: { detail: SessionDetail; theme: Theme }) {
  const zh = Boolean(theme.zh);
  const usage = detail.usage;
  const kinds = presentKinds(usage);
  const total = usage?.total ?? 0;
  return (
    <Card style={{ ...card(theme), gap: 8 }}>
      <CardTitle
        title={zh ? "Token 类型" : "Token kinds"}
        right={zh ? "占比" : "Share"}
        theme={theme}
      />
      {total > 0 && (
        <div
          aria-hidden="true"
          style={{
            display: "flex",
            gap: 1.5,
            height: 6,
            overflow: "hidden",
            borderRadius: 3,
          }}
        >
          {kinds.map(({ kind, value }) => (
            <span
              key={kind}
              style={{
                flex: `${value / total} 1 0`,
                minWidth: 1,
                background: kindColor(theme, kind),
                transition: "flex-grow 240ms cubic-bezier(0.22, 1, 0.36, 1)",
              }}
            />
          ))}
        </div>
      )}
      <div>
        {kinds.map(({ kind, value }, index) => (
          <div
            key={kind}
            title={`${kindNames[kind][zh ? 1 : 0]} ${integer(value)}`}
            style={{
              ...row,
              padding: "6px 0",
              borderTop: index > 0 ? `1px solid ${theme.divider}` : 0,
              fontSize: 12,
            }}
          >
            <ColorKey color={kindColor(theme, kind)}>
              {kindNames[kind][zh ? 1 : 0]}
            </ColorKey>
            <strong style={{ marginLeft: "auto", fontWeight: 600, ...numeric }}>
              {compact(value)}
            </strong>
            <span
              style={{
                width: 36,
                textAlign: "right",
                color: theme.secondary,
                fontSize: 11,
                ...numeric,
              }}
            >
              {share(value, total)}
            </span>
          </div>
        ))}
        {kinds.length === 0 && (
          <span style={{ color: theme.secondary, fontSize: 11 }}>—</span>
        )}
      </div>
    </Card>
  );
}

/** How much the sub-agents added: the session's own part beside theirs, then each sub-agent as a labelled bar. */
function SubagentsCard({
  detail,
  theme,
  onSession,
}: {
  detail: SessionDetail;
  theme: Theme;
  onSession: (id: string) => void;
}) {
  const zh = Boolean(theme.zh);
  const total = detail.usage?.total ?? detail.session.tokens;
  const theirs = detail.subagents?.total ?? 0;
  const own = detail.own?.total ?? total - theirs;
  const children = [...(detail.children ?? [])].sort(
    (a, b) => b.tokens - a.tokens,
  );
  const shown = children.slice(0, 6);
  const rest = children.slice(shown.length);
  const parts = [
    {
      key: "own",
      label: zh ? "会话本身" : "This session",
      value: own,
      color: theme.secondary,
      fade: 1,
    },
    {
      key: "sub",
      label: zh ? "子 agent" : "Sub-agents",
      value: theirs,
      color: theme.secondary,
      fade: 0.4,
    },
  ];
  return (
    <Card style={{ ...card(theme), gap: 9 }}>
      <CardTitle
        title={
          zh
            ? `子 agent（${children.length}）`
            : `Sub-agents (${children.length})`
        }
        right={zh ? "占会话 Token" : "Of the session's tokens"}
        theme={theme}
      />
      {theirs > 0 && (
        <div style={{ display: "grid", gap: 5 }}>
          <div
            aria-hidden="true"
            style={{
              display: "flex",
              gap: 1.5,
              height: 8,
              borderRadius: 4,
              overflow: "hidden",
            }}
          >
            {parts.map((part) => (
              <span
                key={part.key}
                style={{
                  flex: `${total > 0 ? part.value / total : 0} 1 0`,
                  minWidth: part.value > 0 ? 2 : 0,
                  background: part.color,
                  opacity: part.fade,
                  transition: "flex-grow 240ms cubic-bezier(0.22, 1, 0.36, 1)",
                }}
              />
            ))}
          </div>
          <div
            style={{ ...row, justifyContent: "space-between", fontSize: 11 }}
          >
            {parts.map((part) => (
              <span key={part.key} style={{ ...row, gap: 5 }}>
                <span style={{ color: theme.secondary }}>{part.label}</span>
                <strong style={{ fontWeight: 600, ...numeric }}>
                  {compact(part.value)}
                </strong>
                <span style={{ color: theme.secondary, ...numeric }}>
                  {share(part.value, total)}
                </span>
              </span>
            ))}
          </div>
        </div>
      )}
      {shown.length > 0 && (
        <div style={{ borderTop: `1px solid ${theme.divider}`, paddingTop: 8 }}>
          <ShareRows
            rows={[
              ...shown.map((child) => ({
                id: child.id,
                label: (
                  <span style={{ ...row, gap: 6 }}>
                    <span style={clipped}>
                      {child.title || child.modelName || child.model}
                    </span>
                    <span
                      style={{
                        color: theme.secondary,
                        fontSize: 11,
                        flexShrink: 0,
                      }}
                    >
                      {child.modelName || child.model}
                    </span>
                    <ChevronRight
                      size={10}
                      color={theme.tertiary}
                      style={{ flexShrink: 0 }}
                    />
                  </span>
                ),
                value: child.tokens,
                note: share(child.tokens, total),
                color: theme.secondary,
                title: child.title,
              })),
              ...(rest.length > 0
                ? [
                    {
                      id: "__rest__",
                      label: (
                        <span style={{ color: theme.secondary }}>
                          {zh
                            ? `其余 ${rest.length} 个`
                            : `${rest.length} more`}
                        </span>
                      ),
                      value: rest.reduce((sum, child) => sum + child.tokens, 0),
                      note: share(
                        rest.reduce((sum, child) => sum + child.tokens, 0),
                        total,
                      ),
                      color: theme.track,
                    },
                  ]
                : []),
            ]}
            whole={total}
            theme={theme}
            onSelect={(id) => {
              if (id !== "__rest__") onSession(id);
            }}
          />
        </div>
      )}
    </Card>
  );
}

/** Which models did the work: each with its share, cache hits and tokens. */
function ModelsCard({
  detail,
  session,
  theme,
}: {
  detail: SessionDetail;
  session: Session;
  theme: Theme;
}) {
  const zh = Boolean(theme.zh);
  const models = detail.models ?? [];
  const tokensOf = (model: (typeof models)[number]) =>
    model.tokens.total ??
    model.tokens.input +
      model.tokens.output +
      model.tokens.cacheRead +
      model.tokens.cacheWrite +
      model.tokens.reasoning;
  const whole = models.reduce((sum, model) => sum + tokensOf(model), 0);
  return (
    <Card style={{ ...card(theme), gap: 8 }}>
      <CardTitle
        title={zh ? "模型" : "Models"}
        right={
          <>
            <span
              style={{ width: 64, textAlign: "right", whiteSpace: "nowrap" }}
            >
              {zh ? "缓存命中" : "Cache hits"}
            </span>
            <span
              style={{ width: 48, textAlign: "right", whiteSpace: "nowrap" }}
            >
              {zh ? "Token" : "Tokens"}
            </span>
          </>
        }
        theme={theme}
      />
      {models.map((model, index) => {
        const tokens = tokensOf(model);
        const name = model.name ?? model.id ?? session.model ?? "—";
        return (
          <div
            key={`${model.id ?? name}:${index}`}
            style={{ display: "grid", gap: 4 }}
          >
            <div style={{ ...row, fontSize: 12 }}>
              <HUDLogo
                source={model.provider ?? session.source}
                size={13}
                light={theme.light}
              />
              <span style={{ ...clipped, flex: 1 }}>{name}</span>
              <span
                style={{
                  width: 64,
                  textAlign: "right",
                  color: theme.secondary,
                  fontSize: 11,
                  ...numeric,
                }}
              >
                {model.cacheHitRate != null
                  ? `${Math.round(model.cacheHitRate * 100)}%`
                  : "—"}
              </span>
              <strong
                style={{
                  width: 48,
                  textAlign: "right",
                  fontWeight: 600,
                  ...numeric,
                }}
              >
                {compact(tokens)}
              </strong>
            </div>
            <span
              aria-hidden="true"
              style={{
                display: "block",
                height: 4,
                borderRadius: 2,
                background: theme.track,
                overflow: "hidden",
              }}
            >
              <span
                style={{
                  display: "block",
                  height: "100%",
                  width: `${whole > 0 ? (tokens / whole) * 100 : 0}%`,
                  background: theme.input,
                  borderRadius: 2,
                  transition: "width 240ms cubic-bezier(0.22, 1, 0.36, 1)",
                }}
              />
            </span>
          </div>
        );
      })}
      {models.length === 0 && (
        <span style={{ color: theme.secondary, fontSize: 11 }}>—</span>
      )}
    </Card>
  );
}

/** A folded section at the foot of the page. */
function Folded({
  title,
  theme,
  children,
}: {
  title: string;
  theme: Theme;
  children: ReactNode;
}) {
  return (
    <Card style={{ ...card(theme, "0") }}>
      <details className="session-folded">
        <summary
          style={{
            ...row,
            cursor: "pointer",
            padding: "10px 14px",
            fontSize: 13,
            fontWeight: 600,
            listStyle: "none",
          }}
        >
          <ChevronRight
            size={11}
            className="session-folded-chevron"
            style={{
              transition: "transform 180ms cubic-bezier(0.22, 1, 0.36, 1)",
            }}
          />
          {title}
        </summary>
        <div style={{ padding: "0 14px 12px", display: "grid", gap: 8 }}>
          {children}
        </div>
      </details>
    </Card>
  );
}

export function InlineDetail({
  session,
  detail,
  loading,
  error,
  snapshot,
  theme,
  selectedEntry,
  onEntry,
  onSession,
  reveal,
  loadMessages,
  loadingMessages,
}: {
  session: Session;
  detail: SessionDetail | null;
  loading: boolean;
  error: string | null;
  snapshot: Snapshot;
  theme: Theme;
  selectedEntry: string | null;
  onEntry: (id: string | null) => void;
  onSession: (id: string) => void;
  reveal: () => void;
  loadMessages: () => void;
  loadingMessages: boolean;
}) {
  const zh = Boolean(theme.zh);
  const [measure, setMeasure] = useState<Measure>("tokens");
  const client = sessionClient(session, snapshot.providers);
  const now = Date.parse(snapshot.generatedAt);
  const entries = detail?.turns ?? [];
  const byPrompt = detail?.turnsBy === "prompt";
  const picked = entries.find((entry) => entry.id === selectedEntry);
  // Cost bars only when every turn has a list price, so no turn looks free.
  const priced =
    byPrompt &&
    entries.length > 0 &&
    entries.every((entry) => entry.listCost != null);
  const shown: Measure = priced ? measure : "tokens";
  const hasContext = entries.some((entry) => entry.context != null);
  const lastMessage = session.liveFacts?.message?.trim();
  const children = detail?.children ?? [];
  const helped = Boolean(detail?.subagents) || children.length > 0;
  // One model says nothing the figures do not (Open shows the card from two).
  const models = (detail?.models?.length ?? 0) > 1;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <DetailHeader
        session={session}
        detail={detail}
        client={client}
        now={now}
        theme={theme}
        // The runtime's open_session goes back to the session where its client
        // names a place, else shows its log; the label says which.
        reveal={session.navigationTarget || session.logPath ? reveal : null}
        revealLabel={
          session.navigationTarget
            ? zh
              ? "回到会话"
              : "Return to session"
            : snapshot.connection.platform === "macos"
              ? zh
                ? "在访达中显示日志"
                : "Reveal log in Finder"
              : zh
                ? "显示日志"
                : "Reveal log"
        }
      />
      <DetailFigures session={session} detail={detail} theme={theme} />
      {!detail && loading && (
        <div style={{ color: theme.secondary, fontSize: 11 }}>
          {zh ? "正在读取会话用量…" : "Reading session usage…"}
        </div>
      )}
      {error && (
        <div role="alert" style={{ color: theme.secondary, fontSize: 11 }}>
          {error}
        </div>
      )}
      {detail && (
        <>
          <Card style={{ ...card(theme, "12px 14px 10px"), gap: 8 }}>
            <CardTitle
              title={
                !byPrompt
                  ? zh
                    ? "Token 随时间变化"
                    : "Tokens over time"
                  : shown === "cost"
                    ? zh
                      ? "每轮费用"
                      : "Cost per turn"
                    : zh
                      ? "每轮新增 Token"
                      : "New tokens per turn"
              }
              right={
                priced ? (
                  <Tabs
                    value={measure}
                    onValueChange={(value) => setMeasure(value as Measure)}
                  >
                    <TabsList
                      style={{
                        height: 24,
                        background: theme.segment,
                        padding: 2,
                        borderRadius: 6,
                        gap: 1,
                      }}
                      aria-label={zh ? "度量" : "Measure"}
                    >
                      {(["tokens", "cost"] as const).map((value) => (
                        <TabsTrigger
                          key={value}
                          value={value}
                          style={{
                            height: 20,
                            border: 0,
                            fontSize: 11,
                            padding: "2px 10px",
                            borderRadius: 4,
                            color: theme.text,
                            background:
                              measure === value
                                ? theme.selected
                                : "transparent",
                            boxShadow:
                              measure === value && theme.light
                                ? "0 1px 2px rgba(0,0,0,.08)"
                                : "none",
                            transition:
                              "background-color 160ms ease, box-shadow 160ms ease",
                          }}
                        >
                          {value === "tokens" ? "Token" : zh ? "费用" : "Cost"}
                        </TabsTrigger>
                      ))}
                    </TabsList>
                  </Tabs>
                ) : null
              }
              theme={theme}
            />
            <div
              style={{
                ...row,
                justifyContent: "space-between",
                alignItems: "flex-start",
                gap: 12,
              }}
            >
              <ChartLegend entries={entries} measure={shown} theme={theme} />
              {entries.length > 0 && (
                <span
                  style={{
                    color: theme.secondary,
                    fontSize: 11,
                    flexShrink: 0,
                  }}
                >
                  {zh
                    ? "数字为最大的几轮 · 点击一轮看调用"
                    : "Largest turns labelled · click a turn for its calls"}
                </span>
              )}
            </div>
            {entries.length > 0 ? (
              <TurnBarsChart
                entries={entries}
                measure={shown}
                selected={selectedEntry}
                onSelect={(entry) =>
                  onEntry(selectedEntry === entry.id ? null : entry.id)
                }
                hover={(entry) => <TurnHover entry={entry} theme={theme} />}
                theme={theme}
              />
            ) : (
              <div
                style={{
                  display: "grid",
                  height: 120,
                  placeItems: "center",
                  color: theme.secondary,
                  fontSize: 11,
                }}
              >
                {zh ? "暂无已记录的用量" : "No recorded usage"}
              </div>
            )}
            {hasContext && (
              <>
                <div
                  style={{
                    height: 1,
                    background: theme.divider,
                    margin: "2px 0",
                  }}
                />
                <CardTitle
                  title={zh ? "上下文窗口" : "Context window"}
                  right={
                    (zh ? "缓存读取 + 新增输入" : "Cache read + new input") +
                    (detail.contextWindow != null && detail.contextWindow > 0
                      ? (zh ? " · 上限 " : " · limit ") +
                        compact(detail.contextWindow)
                      : "")
                  }
                  theme={theme}
                />
                <ContextChart
                  entries={entries}
                  window={detail.contextWindow}
                  fill={detail.contextFill}
                  selected={selectedEntry}
                  theme={theme}
                />
              </>
            )}
          </Card>
          {picked && (
            <TurnPanel
              key={picked.id}
              entry={picked}
              detail={detail}
              byPrompt={byPrompt}
              theme={theme}
              close={() => onEntry(null)}
            />
          )}
          {/* Two columns: what the tokens were and which models, beside how much sub-agents added. */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))",
              gap: 12,
              alignItems: "start",
            }}
          >
            <div style={{ display: "grid", gap: 12, minWidth: 0 }}>
              <KindsCard detail={detail} theme={theme} />
              {models && helped && (
                <ModelsCard detail={detail} session={session} theme={theme} />
              )}
            </div>
            {helped ? (
              <SubagentsCard
                detail={detail}
                theme={theme}
                onSession={onSession}
              />
            ) : (
              models && (
                <ModelsCard detail={detail} session={session} theme={theme} />
              )
            )}
          </div>
          {lastMessage && (
            <Card style={card(theme)}>
              <CardTitle title={zh ? "最近回复" : "Last reply"} theme={theme} />
              <div
                style={{
                  fontSize: 12,
                  lineHeight: 1.45,
                  whiteSpace: "pre-wrap",
                  userSelect: "text",
                  maxHeight: "18em",
                  overflow: "auto",
                }}
              >
                {lastMessage}
              </div>
            </Card>
          )}
          {detail.messages && detail.messages.length > 0 && (
            <Folded
              title={
                zh
                  ? `对话记录（${detail.messages.length}）`
                  : `Conversation (${detail.messages.length})`
              }
              theme={theme}
            >
              {detail.messageNotice && (
                <p style={{ fontSize: 11, color: theme.secondary, margin: 0 }}>
                  {detail.messageNotice}
                </p>
              )}
              {detail.messages.map((message) => (
                <section
                  key={message.id}
                  style={{
                    borderTop: `1px solid ${theme.divider}`,
                    paddingTop: 8,
                  }}
                >
                  <header
                    style={{
                      fontSize: 11,
                      color: theme.secondary,
                      display: "flex",
                      gap: 8,
                    }}
                  >
                    <strong>{message.role}</strong>
                    {message.timestamp && (
                      <time>
                        {dateFormat(undefined, localeDateTime).format(
                          new Date(message.timestamp),
                        )}
                      </time>
                    )}
                  </header>
                  <div
                    style={{
                      fontSize: 12,
                      lineHeight: 1.5,
                      whiteSpace: "pre-wrap",
                      overflowWrap: "anywhere",
                      userSelect: "text",
                      paddingTop: 5,
                    }}
                  >
                    {formatMessageText(message.text)}
                  </div>
                  {message.toolCall != null && (
                    <details style={{ fontSize: 11, paddingTop: 5 }}>
                      <summary>
                        {zh
                          ? `工具调用 ${message.toolCallId ?? ""}`
                          : `Tool call ${message.toolCallId ?? ""}`}
                      </summary>
                      <pre
                        style={{
                          whiteSpace: "pre-wrap",
                          overflowWrap: "anywhere",
                        }}
                      >
                        {JSON.stringify(message.toolCall, null, 2)}
                      </pre>
                    </details>
                  )}
                </section>
              ))}
              {detail.nextMessageCursor != null && (
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={loadingMessages}
                  onClick={loadMessages}
                >
                  {loadingMessages
                    ? zh
                      ? "读取中…"
                      : "Reading…"
                    : zh
                      ? "加载更多消息"
                      : "Load more messages"}
                </Button>
              )}
              {detail.messagesPending && (
                <p style={{ fontSize: 11, color: theme.secondary, margin: 0 }}>
                  {zh
                    ? "正在索引更多对话内容。"
                    : "More transcript content is being indexed."}
                </p>
              )}
            </Folded>
          )}
          {detail.calls.length > 0 && (
            <Folded
              title={
                zh
                  ? `已记录的调用（${detail.calls.length}）`
                  : `Recorded calls (${detail.calls.length})`
              }
              theme={theme}
            >
              {detail.calls.map((call, index) => (
                <pre
                  key={index}
                  style={{
                    fontFamily: "ui-monospace, monospace",
                    fontSize: 10,
                    color: theme.secondary,
                    whiteSpace: "pre-wrap",
                    overflowWrap: "anywhere",
                    margin: 0,
                    paddingTop: index > 0 ? 8 : 0,
                    borderTop: index > 0 ? `1px solid ${theme.divider}` : 0,
                  }}
                >
                  {typeof call === "string"
                    ? call
                    : JSON.stringify(call, null, 2)}
                </pre>
              ))}
            </Folded>
          )}
        </>
      )}
    </div>
  );
}
