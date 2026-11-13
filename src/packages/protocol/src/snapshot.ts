// What Core broadcasts, and the methods it answers.
import type { Account, BillingAccount, Provider } from "./providers.ts";
import type { RelaySource } from "./prices-relays.ts";
import type { Quota } from "./quota.ts";
import type {
  Approval,
  ClientHooksStatus,
  IslandEvent,
  RequestDelivery,
} from "./requests.ts";
import type { Session } from "./session.ts";
import type { AppSettings, SourcePreferences } from "./settings.ts";

export type Page =
  | "overview"
  | "sessions"
  | "usage"
  | "agents"
  | "requests"
  | "settings"
  | "migration";
/**
 * How far the usage ledger has been read. `kind` is set while the ledger is
 * read through: "first" for a ledger with no logs yet, "reindex" for one
 * read under another data version (the release whose reading rules it holds,
 * such as "0.6.0"). Bytes count every listed log, decoded text for
 * compressed logs.
 */
export interface IndexingStatus {
  complete: boolean;
  pending: number;
  kind?: "first" | "reindex" | null;
  dataVersion?: string;
  bytesDone?: number;
  bytesTotal?: number;
  /**
   * While a run reads: when the oldest log was last written, as far back as
   * the history goes. Null outside a run.
   */
  oldestLogAt?: string | null;
  /**
   * While a run reads: the newest last write among the logs not yet read.
   * Every log written after it has been read; logs are read newest first, so
   * it moves back toward `oldestLogAt`. Null outside a run and once none is
   * left.
   */
  readBackTo?: string | null;
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
export interface TimelinePoint {
  timestamp: string;
  input: number;
  output: number;
  cache: number;
  tokens: number;
}
export interface Capability {
  id: string;
  title: string;
  status: "migrated" | "native" | "blocked" | "planned";
  detail: string;
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
export type NativeMethod =
  | "snapshot"
  | "price_list"
  | "statistics"
  | "quota_history"
  | "plan_value"
  | "sessions"
  | "refresh"
  | "set_setting"
  | "antigravity_cli_status"
  | "set_provider"
  | "answer_request"
  | "session_detail"
  | "open_session"
  | "open_client"
  | "open_link"
  | "open_native"
  | "toggle_hud"
  | "extension"
  | "phone_share"
  | "set_credentials"
  | "remove_credentials"
  | "add_relay_source"
  | "set_relay_kind"
  | "remove_relay_source"
  | "web_sign_out";
