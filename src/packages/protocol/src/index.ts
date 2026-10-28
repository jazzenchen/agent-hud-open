export type Page =
  | "overview"
  | "sessions"
  | "usage"
  | "agents"
  | "requests"
  | "settings"
  | "migration";
export type SessionState = "running" | "waiting" | "idle" | "unverified";
/**
 * How far the usage ledger has been read. `kind` is set while the ledger is
 * read through: "first" for a ledger with no logs yet, "reindex" for one
 * read under another data version (the release whose reading rules it holds,
 * such as "0.6.0"). Bytes count the logs the 31-day retention keeps, decoded
 * text for compressed logs.
 */
export interface IndexingStatus {
  complete: boolean;
  pending: number;
  kind?: "first" | "reindex" | null;
  dataVersion?: string;
  bytesDone?: number;
  bytesTotal?: number;
}

export interface TokenTotals {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  reasoning: number;
  total: number;
  usd: number | null;
  cny: number | null;
}
export interface Quota {
  id: string;
  /** A sampled CLI report, whose freshness follows observedAt. */
  source?: "antigravity_cli";
  /** Stable old-provider identities used by saved account/row visibility. */
  accountId?: string | null;
  legacyWindowId?: string | null;
  vendor: string;
  name: string;
  /** Core's short name for tight places: distinct among the account's
   * windows, else the full name. Shown where the full name does not fit. */
  shortLabel?: string;
  /** Whether this window covers every model, as the account reader reports. */
  allModels?: boolean;
  /** The source's fixed window length; absent/null when it does not report one. */
  windowDurationSeconds?: number | null;
  usedPercent: number | null;
  resetAt: string | null;
  status: "normal" | "warning" | "critical" | "stale";
  account: string | null;
  /** When the window was read; none before its first reading. */
  observedAt?: string | null;
  /** Derived by the runtime for this exact window and its model consumers. */
  metrics?: QuotaMetrics;
}
export type QuotaMeasure = "quota" | "burn" | "tokens" | "consumed" | "cost";
export interface QuotaMetrics {
  /** Percentage points consumed per hour from this cycle's persisted readings. */
  burnRatePctPerHour: number | null;
  tokensPerHour: number | null;
  /** Output tokens / qualifying same-log reply spans, never total tokens / wall time. */
  replyRate: {
    tokensPerSecond: number;
    calls: number;
    tokensOut: number;
    /** No call was timed by its log: spans are gaps between calls, so the rate is at least this. */
    lowerBound?: boolean;
  } | null;
  /** Source tally first; otherwise matching ledger usage within the known cycle. */
  consumed: { used: number; capacity: number | null; unit?: string } | null;
  /** List-price estimate, by currency without conversion; not a subscription charge. */
  cost: { amounts: Record<string, number>; hours: number } | null;
  sampling: { elapsed: number; required: number; afterReset: boolean } | null;
  outlook: {
    kind:
      | "untimed"
      | "exhausted"
      | "no-estimate"
      | "no-usage"
      | "insufficient-data"
      | "exhausts";
    exhaustsAt?: string | null;
    beforeReset?: boolean;
    projectedUsedPercent?: number | null;
  } | null;
  /** Specific missing facts, safe for display; absence must not be interpreted as zero. */
  unavailable?: Partial<Record<QuotaMeasure, string>>;
  /** Start of observed coverage; no usage before this timestamp is inferred. */
  observedFrom?: string | null;
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
export interface TimelinePoint {
  timestamp: string;
  input: number;
  output: number;
  cache: number;
  tokens: number;
}
/** Independent additive categories; input excludes cache writes, output excludes reasoning. */
export type StatisticsTokenKind =
  "cacheWrite" | "input" | "reasoning" | "output" | "cacheRead";
export type StatisticsTokens = Record<StatisticsTokenKind, number>;
export interface StatisticsQuery {
  /** Half-open interval [from, to), RFC3339 UTC timestamps. */
  from: string;
  to: string;
  bucketMinutes: 15 | 30 | 60 | 1440;
  tokenKinds: StatisticsTokenKind[];
  /** Omitted means all; an empty array explicitly selects none. */
  providers?: string[];
  /** Provider-qualified model IDs from the available catalog; omitted means all. */
  models?: string[];
  /** IANA calendar time zone; the backend owns DST-aware day boundaries. */
  timeZone: string;
}
export interface StatisticsCost {
  /** List-price estimates by currency. No exchange-rate conversion or subscription charge. */
  amounts: Record<string, number>;
  pricedTokens: number;
  unpricedTokens: number;
  unpricedModels: string[];
  /** From Core: the unpriced models as people read them, "Claude Code · qwen3-coder:30b". */
  unpricedNames?: string[];
}
export interface StatisticsTotal {
  tokens: StatisticsTokens;
  /** Backend count for exactly the selected token kinds. */
  selectedTokens: number;
  cost: StatisticsCost | null;
}
export interface StatisticsBucket {
  timestamp: string;
  tokens: StatisticsTokens;
  selectedTokens: number;
  /** Backend-selected token counts by provider-qualified model ID. */
  models: Record<string, number>;
}
export interface StatisticsModel extends StatisticsTotal {
  id: string;
  provider: string;
  name: string;
}
export interface StatisticsSession extends StatisticsTotal {
  /** Metadata opens the recorded session; its lifetime counters are not the interval values. */
  session: Session;
}
export interface StatisticsProvider extends StatisticsTotal {
  id: string;
  name: string;
  models: StatisticsModel[];
  topSessions: StatisticsSession[];
  /** From Core (Open's AgentUsage.cacheReadShare): the share of the list price spent reading context back from the cache, 0…1; null when no model has a price. */
  cacheReadShare?: number | null;
  /** From Core (Open's AgentUsage.recachedCost): what sending context again after the cache lapsed cost at list prices, US dollars; null when none was. */
  recachedCost?: number | null;
  series: number[];
}
export interface StatisticsCoverage {
  state: "complete" | "partial" | "unavailable";
  partial: boolean;
  observedFrom: string | null;
  observedTo: string | null;
  pendingSources: number;
  unattributedEvents: number;
  unavailableProviders: string[];
  notice: string | null;
}
export interface StatisticsPeriod {
  id: "today" | "7d" | "30d";
  from: string;
  to: string;
  totals: StatisticsTotal;
  models: StatisticsModel[];
  coverage: StatisticsCoverage;
}
export interface StatisticsHeatmap {
  from: string;
  to: string;
  /** Monday-first 7 × 24 local calendar cells; values already use the selected kinds. */
  tokens: number[][];
  intensity: number[][];
  models: Record<string, number>[][];
  coverage: StatisticsCoverage;
}
export interface StatisticsResponse {
  kind: "statistics";
  dataRevision: number;
  generatedAt: string;
  query: StatisticsQuery;
  totals: StatisticsTotal;
  buckets: StatisticsBucket[];
  providers: StatisticsProvider[];
  models: StatisticsModel[];
  available: {
    providers: { id: string; name: string }[];
    models: { id: string; provider: string; name: string }[];
  };
  coverage: StatisticsCoverage;
  recent: StatisticsPeriod[];
  heatmap: StatisticsHeatmap;
}
export interface Provider {
  id: string;
  name: string;
  detected: boolean;
  enabled: boolean;
  notice: string | null;
  sessions: number;
  /** From Core: what the client's calls in the last 24 hours used and cost at list prices, money by currency; null when it made none. */
  recent?: {
    tokens: number;
    costs: Record<string, number>;
    unpricedTokens: number;
  } | null;
  accountStatus?: {
    status: "loading" | "ready" | "error" | "unavailable";
    checkedAt: string | null;
    notice: string | null;
    configurableFields?: string[];
    configured?: boolean;
  };
  /** Finding an installed/configured client does not imply its data adapter is ported. */
  dataStatus?: "supported" | "account-only" | "detection-only";
  discovery?: {
    installed: boolean;
    configured: boolean;
    usage: boolean;
    /** Non-sensitive evidence categories; never tokens, config contents or personal paths. */
    evidence: string[];
  };
}
export interface Account {
  id: string;
  /** A sampled CLI report, whose freshness follows checkedAt. */
  source?: "antigravity_cli";
  provider: string;
  label: string | null;
  plan: string | null;
  checkedAt: string | null;
  /** The last read failed or the client's last report is no longer fresh. */
  stale: boolean;
  /** From Core: the label, else "Account" and the id's end. */
  name?: string;
  /** From Core: the plan in the provider's tier names ("Max x20"). */
  planLabel?: string | null;
  /** From Core: included in the current account inventory. */
  current?: boolean;
  /** From Core: why the last read failed, from its client; shown in amber. */
  warning?: string | null;
  /** From Core: what the client said otherwise. */
  notice?: string | null;
  /** Codex's earned usage resets, from the account's rate limits. */
  resetCredits?: ResetCredits | null;
}
export interface ResetCredits {
  availableCount: number;
  /** Earliest expiry first; a credit without a known expiry has none. */
  credits?: { id: string; expiresAt: string | null }[];
}
export interface RequestAnswer {
  selected: string[];
  custom?: string | null;
}
export type RequestDecision =
  | { kind: "allow" | "deny" | "leave" }
  | { kind: "always"; suggestion?: unknown }
  | { kind: "answer"; answers: Record<string, RequestAnswer> }
  | { kind: "approve_plan"; acceptEdits: boolean }
  | { kind: "keep_planning"; message?: string };
export interface ApprovalQuestion {
  id: string | null;
  answerKey: string;
  question: string;
  header: string | null;
  options: { label: string; description: string | null }[];
  multiSelect: boolean;
}
export interface Approval {
  id: string;
  title: string;
  source: string;
  toolName: string;
  input: string;
  sessionId: string | null;
  createdAt: string;
  allowsAlways: boolean;
  kind?: "permission" | "question" | "plan";
  vendor?: string;
  summary?: string;
  detail?: string | null;
  cwd?: string | null;
  path?: string | null;
  removed?: string | null;
  added?: string | null;
  suggestions?: unknown[];
  /** From Core: what always allowing adds, as text (a rule, or a website). */
  alwaysAllowRule?: string | null;
  /** From Core: the allow button's words where they are not "Allow once",
   * such as "Allow this conversation" for a native approval kept per session. */
  allowLabel?: string | null;
  /** From Core: the request's one-word kind where Core names it (APP, WEB). */
  badge?: string | null;
  questions?: ApprovalQuestion[];
  plan?: string | null;
  answersQuestion?: boolean;
  /** From Core: why a question can be answered only in the client's own
   * window, such as a Claude desktop box whose session Agent HUD cannot
   * reach; null when it can be answered here. */
  answerNote?: string | null;
  /** From Core, with `answerNote`: the step that would let Agent HUD answer
   * it here; null when nothing on this Mac would. */
  answerFix?: AnswerFix | null;
  answersPlan?: boolean;
}
/** What lets Agent HUD answer a client's questions: install the client's
 * Agent HUD plugin (Settings → Agent plugins), load it in the session that
 * asks (`/reload-plugins`), or turn answering back on in Settings → Agent
 * plugins. */
export type AnswerFix = "installPlugin" | "reloadPlugins" | "turnOnAnswers";
export type ApprovalWaitMinutes = 1 | 3 | 5 | 10 | 30 | 60;
export interface ClientHooksStatus {
  supported: boolean;
  /** The callback receiver is listening; this alone does not prove client installation. */
  enabled: boolean;
  desired?: boolean;
  installed?: boolean;
  installable?: boolean;
  notice?: string | null;
  clients?: {
    source: string;
    installed: boolean;
    active: boolean;
    notice: string | null;
  }[];
}
export interface RequestDelivery {
  id: string;
  source: string;
  sessionId: string | null;
  state: "confirmed" | "failed" | "uncertain";
  reason: string | null;
  at: string;
}
export interface Capability {
  id: string;
  title: string;
  status: "migrated" | "native" | "blocked" | "planned";
  detail: string;
}
export interface PanelBlockLayout {
  widget?: "bar" | "ring" | "number" | "chart";
  measures?: QuotaMeasure[];
  hidden?: boolean;
  hiddenRows?: string[];
  hiddenAccounts?: string[];
  rows?: string[];
  accounts?: string[];
  countdown?: boolean;
  columns?: number;
  ringSize?: "small" | "medium" | "large";
  span?: "full" | "half";
  period?: "day" | "week" | "month";
}
export interface PanelLayout {
  order?: string[];
  sections?: ("blocks" | "tokens" | "sessions")[];
  hiddenSections?: ("blocks" | "tokens" | "sessions")[];
  countdown?: boolean;
  blocks?: Record<string, PanelBlockLayout>;
  style?: {
    spacing?: "compact" | "regular" | "roomy";
    separators?: boolean;
    width?: number;
  };
  sessions?: {
    limit?: number;
    period?: "day" | "week";
    agents?: string[];
    projects?: string[];
  };
  tokens?: {
    period?: "day" | "week" | "month";
    agents?: string[];
    models?: string[];
  };
  hud?: { agents?: string[] };
}
export interface SourcePreferences {
  live?: boolean;
  reminders?: boolean;
}
export interface AppSettings {
  /** Display quota percentages as used or available; readings remain usedPercent. */
  quotaDisplay?: "consumed" | "remaining";
  /** Additional Codex CLI homes whose signed-in accounts should be read. */
  codexHomes?: string[];
  /** CLI settings files connected to Antigravity's account status callback. */
  antigravityCliSettings?: string[];
  appearance: "dark" | "light" | "system";
  language: string;
  clientHooks: boolean;
  readCopilotQuota?: boolean;
  /** The relay hosts whose balance may be read with the key the client already sends them. */
  relayReading?: string[];
  /** The owner's prices per million tokens, by model as recorded (`model` or `model#route`). */
  customPrices?: Record<string, CustomPrice>;
  /** By relay host: times the vendor's list price, when the relay has no price list. */
  relayMultipliers?: Record<string, number>;
  /** By local host: a model server (free) or a gateway to a paid API. */
  localKinds?: Record<string, "model" | "gateway">;
  approvalWaitMinutes?: ApprovalWaitMinutes;
  launchAtLogin: boolean;
  appearanceFollowsScreen?: boolean;
  animations: boolean;
  hudVisible: boolean;
  showMenuBarIcon?: boolean;
  hudTools?: boolean;
  /** Answer the Claude desktop app's questions on the HUD through Agent HUD's Claude Code plugin; on unless turned off. */
  claudeAnswers?: boolean;
}
/** Acquisition status belongs to the selected native adapter, not the UI. */
export interface PlatformCapabilities {
  os: "macos" | "windows" | "linux";
  validation: "macos-live-verified" | "unverified";
  clientDiscovery: "implemented";
  portableAccountReaders: Record<
    "claude" | "codex" | "cursor" | "deepseek",
    "live-verified" | "unverified"
  >;
  antigravityLocalService: "live-verified" | "unverified" | "unsupported";
  legacyMacHistory: boolean;
  scripts: "build-time-embedded";
}
export interface IslandEvent {
  id: string;
  kind: "completion" | "quota-exhaustion" | "quota-reset" | "reset-credits";
  at: string;
  startedAt?: string | null;
  source: string;
  sessionId?: string;
  turnId?: string;
  title?: string;
  model?: string;
  message?: string | null;
  quotaId?: string;
  accountId?: string;
  usedPercent?: number;
  resetAt?: string;
  crossing?: "critical" | "exhausted" | "forecast";
  timeToExhaustSeconds?: number;
  otherExhaustedWindows?: { id?: string; name?: string }[];
  added?: number;
  availableCount?: number;
  newCredits?: { id?: string; expiresAt?: string | null }[] | number;
  islandAlert?: boolean;
  navigationTarget?: unknown;
}
export interface Snapshot {
  /** The interface language Core resolved from the Language setting and the
   * system's preferred language; Core writes its own text in it. */
  language?: "zh" | "en";
  status?: { paused?: boolean };
  sourcePreferences?: Record<string, SourcePreferences>;
  islandEvents?: { revision: number; items: IslandEvent[] };
  schemaVersion: 1;
  generatedAt: string;
  /** Changes when the persisted usage facts or canonical selection change. */
  statisticsRevision?: number;
  /** Changes when session facts, canonical selection or rolling scope membership change. */
  sessionsRevision?: number;
  /** The broadcast contains HUD-relevant sessions; history is queried separately. */
  sessionScope?: "recent";
  /** Unread history stays incomplete even while a failed read is paused. */
  indexing?: IndexingStatus;
  mode: "live" | "demo";
  connection: {
    status: "connected" | "disconnected" | "loading";
    platform: string;
    message: string | null;
    platformCapabilities?: PlatformCapabilities;
    clientHooks?: ClientHooksStatus;
    system?: {
      platform: string;
      launchAtLogin: {
        supported: boolean;
        enabled: boolean;
        status: "enabled" | "disabled" | "requires-approval" | "unavailable";
        notice: string | null;
      };
    };
    localTools?: {
      supported: boolean;
      enabled: boolean;
      listening: boolean;
      notice: string | null;
    };
  };
  totals: TokenTotals;
  quotas: Quota[];
  sessions: Session[];
  timeline: TimelinePoint[];
  providers: Provider[];
  accounts?: Account[];
  approvals: Approval[];
  /** Native client receipts; queue acceptance alone is not delivery confirmation. */
  requestDeliveries?: RequestDelivery[];
  settings: AppSettings;
  capabilities: Capability[];
  billing: BillingAccount[];
  /** From Core: the relays the clients are configured with, for consent in Settings. */
  relays?: RelaySource[];
}
/** A price per million tokens; an empty cache rate takes the input's. */
export interface CustomPrice {
  currency: "USD" | "CNY";
  input: number;
  output: number;
  cacheRead: number | null;
  cacheWrite: number | null;
}
/** One model used in the last 30 days, on the price page. */
export interface PriceRow {
  id: string;
  source: string;
  client: string;
  /** The model as recorded, with its route: the key of its custom price. */
  model: string;
  name: string;
  route: string | null;
  routeKind: "local-server" | "cloud" | "relay" | "local" | "provider" | null;
  tokens: number;
  /** Per million tokens at the base rates; null when nothing prices it. */
  price: {
    origin: "custom" | "local" | "relay" | "multiplier" | "catalog";
    currency: string;
    input: number;
    output: number;
    cacheRead: number;
    cacheWrite: number;
  } | null;
  custom: CustomPrice | null;
}
/** From Core (`price_list`): the price page. */
export interface PriceList {
  kind: "prices";
  models: PriceRow[];
  relays: { host: string; multiplier: number | null; listed: boolean }[];
  localHosts: { host: string; kind: "model" | "gateway" | null }[];
}
/** A relay, one per address: found in Claude Code's settings env or Codex's providers, or added by hand. */
export interface RelaySource {
  /** The relay's host. */
  provider: string;
  product: "relay";
  /** The clients configured with it. */
  clients: string[];
  /** Who uses it: those clients, then the agents whose calls went through it in the last 30 days. */
  agents: string[];
  consented: boolean;
  /** What it last reported, in its own money. */
  balance?: { label: string; amount: number; currency: string };
  /** Which template answered: newapi, sub2api, openrouter, custom. */
  kind?: string;
  /** Why no balance was read. */
  notice?: string;
  /** No template fit; the owner may choose one (`set_relay_kind`). */
  unrecognized?: boolean;
  /** The template the owner chose for this address, and a custom one's endpoint. */
  chosen?: RelayKind;
  custom?: RelayCustom;
  checkedAt?: string | null;
  /** Added by hand in Settings, with its id and the name given, if any. */
  manual?: boolean;
  id?: string;
  name?: string;
}
/** How a relay's balance is read: sniffed, one of the common templates, or the owner's own endpoint. */
export type RelayKind = "auto" | "newapi" | "sub2api" | "openrouter" | "custom";
/** A balance endpoint on the relay's own address, and where the balance is in its answer. */
export interface RelayCustom {
  url: string;
  remaining: string;
  divisor: number;
  currency: "USD" | "CNY";
  header?: string | null;
}
export interface BillingPeriod {
  from: string;
  to: string;
  hours: number;
  cost: { currency: string; amount: number } | null;
  burnRate: { currency: string; perDay: number } | null;
  runsOutAt: string | null;
  replyRate: {
    tokensPerSecond: number;
    calls: number;
    tokensOut: number;
    lowerBound?: boolean;
  } | null;
  consumed: { used: number; capacity: null; unit: "tokens" } | null;
  tokensPerHour: number | null;
  series: { timestamp: string; amount: number; currency: string }[];
  coverage: { complete: boolean; notice: string | null };
}
export interface BillingAccount {
  id: string;
  accountId?: string | null;
  vendor: string;
  label: string;
  amount: number;
  currency: string;
  notice: string | null;
  /** From Core (Open's AlertPolicy.balanceLevel): critical at or below zero or while the service refuses the balance, warning at or below its currency's line. */
  level?: "normal" | "warning" | "critical";
  metrics?: {
    periods: Partial<Record<"day" | "week" | "month", BillingPeriod>>;
  };
}
export interface SessionMessage {
  id: string;
  parentId: string | null;
  role: string;
  text: string;
  timestamp: string | null;
  toolCallId: string | null;
  toolCall: unknown;
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
}
export type NativeMethod =
  | "snapshot"
  | "price_list"
  | "statistics"
  | "sessions"
  | "refresh"
  | "set_setting"
  | "antigravity_cli_status"
  | "set_provider"
  | "answer_request"
  | "session_detail"
  | "open_session"
  | "open_native"
  | "toggle_hud"
  | "extension"
  | "phone_share"
  | "set_credentials"
  | "remove_credentials"
  | "add_relay_source"
  | "set_relay_kind"
  | "remove_relay_source";
