// The app's settings and the HUD panel's layout.
import type { CustomPrice } from "./prices-relays.ts";

export type ApprovalWaitMinutes = 1 | 3 | 5 | 10 | 30 | 60;
export interface PanelBlockLayout {
  /** `rings` are concentric (年轮) and `segments` split one ring (分段环), one
   * per account; a layout saved before them says `ring`, read as `rings`. */
  widget?: "bar" | "rings" | "segments" | "ring" | "number" | "chart";
  hidden?: boolean;
  hiddenRows?: string[];
  hiddenAccounts?: string[];
  rows?: string[];
  accounts?: string[];
  countdown?: boolean;
  columns?: number;
  ringSize?: "small" | "medium" | "large";
  /** false draws `rings` and `segments` as the ring alone, the most used
   * window's figure inside, the header and picker kept. */
  legend?: boolean;
  /** How much of a line it takes; blocks asking the same share one where each
   * keeps enough room. */
  span?: "full" | "half" | "third" | "quarter" | "fifth" | "sixth";
  period?: "day" | "week" | "month";
}
/** The panel's own sections. The quota history and the plan value show only
 * once `sections` names them (`addedSections`). */
export type PanelSectionId =
  "blocks" | "tokens" | "sessions" | "quotaHistory" | "planValue";
/** What the quota history or the plan value in the panel shows. */
export interface PanelFilterLayout {
  /** Only these agents' windows or plans. */
  agents?: string[];
  /** Model ids whose windows (once every one spending a window is hidden)
   * or calls are left out. */
  hiddenModels?: string[];
}
export interface PanelLayout {
  order?: string[];
  sections?: PanelSectionId[];
  hiddenSections?: PanelSectionId[];
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
    /** Model IDs hidden from the chart; unseen models stay visible by default. */
    hiddenModels?: string[];
    /** Preferred model order; newly observed models follow in their natural order. */
    modelOrder?: string[];
  };
  quotaHistory?: PanelFilterLayout & { period?: "day" | "week" | "month" };
  planValue?: PanelFilterLayout;
  hud?: { agents?: string[] };
  /** Controls an agent added from get_hud's catalog, by id; `sections` and `order`
   * place them among the panel's sections or its blocks. */
  custom?: Record<string, PanelNode>;
}
/** One control of an agent's own: its type, its fields and, for a row, column,
 * grid or repeat, its children. */
export interface PanelNode {
  id?: string;
  type: string;
  children?: PanelNode[];
  [field: string]: unknown;
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
  /** Additional Claude Code CLAUDE_CONFIG_DIR folders whose signed-in accounts should be read. */
  claudeHomes?: string[];
  /** CLI settings files connected to Antigravity's account status callback. */
  antigravityCliSettings?: string[];
  appearance: "dark" | "light" | "system";
  language: string;
  clientHooks: boolean;
  /** Client ids whose approval hook goes in only once the user turns it on
   * (Copilot CLI's, which Copilot asks before its own rules); it goes in
   * while client hooks are on. */
  optInApprovals?: string[];
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
