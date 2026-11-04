// Sessions as lists and the HUD show them, and the Sessions page's query.
import type { StatisticsTokens } from "./statistics.ts";

export type SessionState = "running" | "waiting" | "idle" | "unverified";
/** A local Claude Desktop Code Project thread. */
export interface DesktopProjectSession {
  execution: "local" | "unknown";
  status: "unknown";
  /** Core's localized status label. */
  label: string;
  /** Core's localized Project worker label. */
  kindLabel: string;
  /** Human-readable Project name. */
  projectLabel?: string;
  /** Core's localized name for the native return action. */
  openLabel: string;
  /** False when the local origin cannot be identified safely. */
  openAvailable: boolean;
  /** Related local executors, retained separately for usage and detail. */
  localWorkerCount?: number;
}
export interface Session {
  id: string;
  title: string;
  source: string;
  model: string;
  /** The model as people read it ("Opus 4.5"), from Core; `model` is the logged id. */
  modelName?: string | null;
  /** From Core: the vendor the session's calls are listed under, as Open names it ("Claude", "GitHub Copilot"). */
  vendorName?: string;
  cwd: string | null;
  /** From Core: the client that drives the session ("Codex Desktop", "Claude Code IDE extension"). */
  client?: string | null;
  /** From Core: where the session's first (`init`) and latest prompts were typed, as `category:source` tags ("cli:iterm", "desktop:claude", "mobile:iphone"). Categories: desktop, cli, ide, mobile, mcp, sdk, others; a source nobody can tell is `others`. Null when unknown. */
  origins?: { init: string | null; latest: string | null };
  /** From Core: the project id, the repository its worktrees share written with ~ ("unassigned" filters match none). */
  project?: string | null;
  startedAt?: string | null;
  updatedAt: string;
  /** From Core: when the session's last message was written, the person's
   * prompt or the agent's text reply; the last event where the client
   * records no messages. Lists order sessions by it. */
  lastMessageAt?: string | null;
  /** From Core, for its own work, agents and the phone; no list, filter or
   * label shows it (owner, 2026-10-26). */
  state: SessionState;
  /** From Core: the session's tokens with its sub-agents' at any depth (decided 2026-10-21, as Open); a sub-agent's own row still counts once in every total. */
  tokens: number;
  turnCount?: number | null;
  /** From Core, on the Sessions page's rows (`turnBars: true`): the newest turns a row draws. */
  turnBars?: SessionTurnBars;
  /** From Core, on the Sessions page's rows: the latest own call's prompt. */
  contextTokens?: number | null;
  /** From Core, on the Sessions page's rows: contextTokens / the model's window, 0…1. */
  contextFill?: number | null;
  /** From Core: `tokens` by kind, sub-agents' included. */
  usage?: StatisticsTokens & { total: number };
  /** From Core: what the calls, sub-agents' included, cost at API list prices, in US dollars; null when a call has no list price. */
  cost: number | null;
  /** From Core: what the calls with a list price cost, sub-agents' included, in US dollars; null when none has one. */
  pricedCost?: number | null;
  /** From Core: the tokens of the calls with no list price, sub-agents' included. */
  unpricedTokens?: number;
  /** From Core: a DeepSeek session billed to a linked account, at the account's prices; null when a call has no price. Absent for other sessions. */
  accountCost?: { amount: number; currency: string } | null;
  contextPercent: number | null;
  logPath: string | null;
  parentId: string | null;
  navigationTarget?: unknown;
  /** Present for Claude Desktop Code Project threads. */
  desktopProject?: DesktopProjectSession;
  /** Authoritative client events, when the provider supplies them; absent is unknown. */
  liveFacts?: SessionLiveFacts;
}
/** The kinds a turn adds, as the charts stack them from the base up. */
export type NewTokenKind = "cacheWrite" | "input" | "reasoning" | "output";
/** From Core (agent-hud-native-open `TurnSparkline`): a session's newest turns (at most 64), or its 15-minute periods when its log marks no prompts. */
export interface SessionTurnBars {
  by: "prompt" | "period";
  count: number;
  /** The session's own new tokens per bar, one list per kind present; null when none. */
  own: Partial<Record<NewTokenKind, number[]>> | null;
  /** The sub-agents' new tokens per bar, the same way; null when no sub-agent helped. */
  subagents: Partial<Record<NewTokenKind, number[]>> | null;
  /** Positions of the bars whose turn was compacted. */
  compacted: number[];
}
export interface AttentionFact {
  id: string;
  sessionId: string;
  kind: "permission" | "question" | "plan";
  at: string;
}
export interface SessionLiveFacts {
  turnId: string | null;
  turnStartedAt: string | null;
  outcome: "completed" | "interrupted" | "failed" | "limited" | null;
  endedAt: string | null;
  message?: string | null;
  limitedUntil: string | null;
  observedAt: string;
  lastEventAt: string;
  activity: {
    work: "shell" | "command" | "edit" | "read" | "agents" | "question" | null;
    openCall: {
      work:
        "shell" | "command" | "edit" | "read" | "agents" | "question" | null;
      at: string;
    } | null;
    toolAt: string | null;
    delegating: boolean;
    awaitsModel: boolean;
    retrying: boolean;
    compactingSince: string | null;
  } | null;
  requests: AttentionFact[];
}
export interface SessionsQuery {
  offset?: number;
  /** Defaults to 100; the runtime accepts at most 200. */
  limit?: number;
  /** Recent is seven days, active 24 hours, of activity; all is every retained session. */
  scope?: "recent" | "active" | "all";
  source?: string;
  /** Complete-index filters; omitted means all, an empty list selects none. */
  sources?: string[];
  projects?: string[];
  /** Omitted means all projects; null means sessions without a project. */
  project?: string | null;
  search?: string;
  rootsOnly?: boolean;
  /** Rows also carry turnCount, turnBars, contextTokens and contextFill (the Sessions page's rows). */
  turnBars?: boolean;
  /** Direct children of this parent; overrides rootsOnly. */
  parentId?: string;
  /** IANA calendar time zone the response's `days` are cut in, DST-aware; UTC when omitted. */
  timeZone?: string;
}
/** From Core: what the sessions of one local day spent, every matching session, not only a page's. */
export interface SessionsDay {
  /** When the day began, RFC3339; null for Earlier: before the six days ahead of today, or with no time. */
  start: string | null;
  sessions: number;
  /** The sessions' tokens, sub-agents' included, as their rows count them. */
  tokens: number;
  /** The same by kind; null when no session has a breakdown. */
  usage: (StatisticsTokens & { total: number }) | null;
  /** Money as a row has it: the whole cost when every session has one, else null. */
  cost: number | null;
  /** The sessions' whole costs, else their priced calls', in US dollars; null when none has one. */
  pricedCost: number | null;
  /** The tokens of the calls with no list price, of the sessions with no whole cost. */
  unpricedTokens: number;
  /** The five sessions that spent the most tokens, most first, and the others that spent any (the Today card's ranking). */
  top: { id: string; title: string; tokens: number }[];
  rest: { count: number; tokens: number };
}
export interface SessionsResponse {
  kind: "sessions";
  revision: number;
  generatedAt: string;
  /** One page, newest last message first, then source, ID and path. */
  items: Session[];
  total: number;
  nextOffset: number | null;
  /** Scope/root facets before source, project and search filters are applied. */
  sources: string[];
  projects: (string | null)[];
  /** Readable labels for stable local Project keys. */
  projectLabels?: Record<string, string>;
  /** Each local day the matching sessions' last messages fall on (a later one under today), newest first, Earlier last. The days follow `items`: the first day's `sessions` are the first items, the next day's the next. */
  days: SessionsDay[];
}
