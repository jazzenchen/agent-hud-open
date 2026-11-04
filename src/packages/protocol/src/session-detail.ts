// One session's page: its turns, calls, sub-agents and messages.
import type { Session } from "./session.ts";
import type { StatisticsTokens } from "./statistics.ts";

export interface SessionMessage {
  id: string;
  parentId: string | null;
  role: string;
  text: string | null;
  timestamp: string | null;
  toolCallId: string | null;
  toolCall: unknown;
  /** From Core: the user's own words, neither context a client hands the model nor its note of an interruption. */
  isPrompt: boolean;
}
export interface SessionCall {
  id?: string;
  turnId?: string;
  turnIndex?: number;
  index?: number;
  timestamp: string;
  model?: string | null;
  provider?: string | null;
  subagent?: boolean;
  compacted?: boolean;
  input: number;
  output: number;
  cacheRead?: number;
  cacheWrite?: number;
  cache?: number;
  reasoning?: number;
  total?: number;
  context?: number | null;
  message?: string | null;
  /** From Core: the tools the call asked for, in order (Open's TurnCall.tools). */
  tools?: string[];
  /** From Core: the session's own log rather than a sub-agent's. */
  own?: boolean;
  /** From Core: the log the call was recorded in; a sub-agent's matches its `SessionTurnAgent.log`. */
  log?: string;
  modelName?: string | null;
  source?: string;
  /** From Core: prompt tokens the call sent again because the cache lapsed. */
  recached?: number | null;
  /** From Core: the call at API list prices, US dollars. */
  listCost?: number | null;
  /** From Core: what the call added: every kind but cache reads. */
  newTokens?: number;
}
/** From Core: what a picked turn's calls add up to (agent-hud-native-open's line above them). */
export interface SessionTurnSummary {
  calls: number;
  subagentCalls: number;
  subagentLogs: number;
  /** From the first call to the last. */
  spanMs: number;
  /** US dollars at API list prices; null unless every call has one. */
  listCost: number | null;
  /** New tokens the calls added (cache reads excluded). */
  added: number;
  /** The call that added the most, with more than one call. */
  largestNew: number | null;
  largestShare: number | null;
}
/** From Core: a sub-agent with calls in a picked turn, for its timeline. */
export interface SessionTurnAgent {
  /** The sub-agent's log; its calls carry the same `log`. */
  log: string;
  /** What the agent was sent to do (Claude Code's description), else its log's title. */
  name: string;
  /** The client's agent type (Claude Code's `agentType`), when it records one. */
  kind: string | null;
  /** The log that started it: the session's own or another sub-agent's. */
  parent: string | null;
  startedAt: string | null;
  lastAt: string | null;
  running: boolean;
}
/** Tokens as the session page counts them. */
export type SessionTokens = StatisticsTokens & { total: number };
/** What WorkBuddy's own metering charged a session (Open's SessionCredits); text fields are two decimals. */
export interface SessionCredits {
  total: number;
  text: string;
  unattributed: number;
  unattributedText: string | null;
  attributedShare: number;
  byModel: Record<string, number>;
  used: number | null;
  size: number | null;
}
export interface SessionDetail {
  /** From Core: "prompt" when turns are prompts, "period" for 15-minute periods (logs that mark no prompts). Absent: the parser's usage entries. */
  turnsBy?: "prompt" | "period";
  /** From Core: turns with calls, older ones than `turns` keeps (the newest 200) included. */
  turnCount?: number;
  /** From Core: model calls recorded, sub-agents' included. */
  callCount?: number;
  /** From Core: the latest own call's prompt (fresh, cache writes, cache reads). */
  contextTokens?: number | null;
  /** From Core: contextTokens / contextWindow, 0…1. */
  contextFill?: number | null;
  /** From Core: cache reads as a share of every prompt; null when nothing was prompted. */
  cacheHitRate?: number | null;
  /** From Core: the session's own log apart from its sub-agents'; null when no sub-agent spent any. */
  own?: SessionTokens | null;
  /** From Core: the sub-agents' part of the session's tokens (at any depth); already in `usage`, the turns and the models. Null when no sub-agent spent any. */
  subagents?: SessionTokens | null;
  /** From Core: the breakdown at API list prices, US dollars; null when a call has no list price. */
  listCost?: number | null;
  /** From Core: what the calls with a list price cost, US dollars; null when none has one. */
  pricedCost?: number | null;
  /** From Core: the tokens of the calls with no list price. */
  unpricedTokens?: number;
  credits?: SessionCredits | null;
  /** From Core, for a requested turn: its calls, the session's and its sub-agents', and what they add up to. */
  turnCalls?: {
    turn: string;
    calls: SessionCall[];
    summary?: SessionTurnSummary;
    agents?: SessionTurnAgent[];
  };
  startedAt?: string | null;
  updatedAt?: string | null;
  contextWindow?: number | null;
  models?: {
    id?: string;
    agentId?: string;
    provider?: string;
    name?: string;
    tokens: StatisticsTokens & { total?: number };
    /** From Core: the model's cache reads as a share of its prompts; null when it was prompted nothing. */
    cacheHitRate?: number | null;
    /** From Core: whether the model, used through this client, has a list price. */
    priced?: boolean;
  }[];
  /** From Core: the session's tokens by kind, sub-agents' included; the same as its row's. */
  usage?: StatisticsTokens & { total: number };
  messages?: SessionMessage[];
  nextMessageCursor?: number | null;
  messagesPending?: boolean;
  messageNotice?: string;
  messagesComplete?: boolean;
  session: Session;
  /** Authoritative relationships, including sessions outside the broadcast's recent scope. */
  parent?: Session | null;
  children?: Session[];
  turns: {
    id: string;
    index: number;
    timestamp: string;
    input: number;
    output: number;
    cache: number;
    cacheRead?: number;
    cacheWrite?: number;
    reasoning?: number;
    context: number | null;
    compacted?: boolean;
    subagent?: boolean;
    model?: string | null;
    calls?: SessionCall[];
    message: string | null;
    /** From Core: the turn's (or period's) last call. */
    end?: string;
    /** From Core: the turn's tokens by kind, sub-agents' included. */
    kinds?: SessionTokens;
    /** From Core: the session's own part of `kinds`. */
    own?: SessionTokens;
    /** From Core: what the turn added: every kind but cache reads. */
    newTokens?: number;
    /** From Core: the sub-agents' part of `newTokens`. */
    subagentNew?: number | null;
    total?: number;
    callCount?: number;
    /** From Core: the turn's largest own prompt, when above the context it ended with. */
    peakContext?: number | null;
    /** From Core: prompt tokens sent again because the cache lapsed. */
    recached?: number | null;
    /** From Core: the turn at API list prices, US dollars; null when a call has no list price. */
    listCost?: number | null;
    /** From Core: what the turn's calls with a list price cost, US dollars; null when none has one. */
    pricedCost?: number | null;
    /** From Core: the tokens of the turn's calls with no list price. */
    unpricedTokens?: number;
    /** From Core: what sub-agents spent in the turn. */
    subagents?: SessionTokens | null;
    /** From Core: the same part in the chart's `input`, `output` and `cache`, which the chart fades. */
    subagentParts?: { input: number; output: number; cache: number } | null;
  }[];
  calls: unknown[];
}
