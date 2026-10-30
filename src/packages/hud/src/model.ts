import hudDefaults from "@agent-hud/protocol/hud-defaults.json";
import type {
  PanelLayout,
  Quota,
  RequestDecision,
  Snapshot,
} from "@agent-hud/protocol";
import { agentArtworkKey, agentVendorName } from "./agent-artwork";
import { restingAgents } from "./resting-agents.ts";
import { recentLabel, recentUsage } from "./recent-usage.ts";
import { visibleBalances } from "./balance-rows.ts";
import type { GlowEffect, GlowStyle } from "./HUDGlow";
import {
  moneyAmount,
  snapshotLanguage,
  quotaPercentLabel,
  type QuotaDisplay,
} from "@agent-hud/ui/format";
export { agentVendorName } from "./agent-artwork";

export type HUDMode = "notch" | "logos" | "simple";
export type HUDEdge = "top" | "right" | "bottom" | "left";
export type HUDDecision = RequestDecision;

/** Per-screen placement, with the same defaults and limits as ScreenPlacement.swift. */
/** A newer version of the app that the product found: the host's
 * `UpdateNotice`, which every HUD panel shows as an arrow in its footer. */
export interface HUDUpdateNotice {
  version: string;
  settingsPage: string;
}

export interface HUDConfig {
  panelLayout: PanelLayout;
  mode: HUDMode;
  glowStyle: GlowStyle;
  glowEffect: GlowEffect;
  breathSeconds: number;
  idleBreathSeconds: number;
  breathAmplitude: number;
  glowRange: number;
  glowBlur: number;
  glowBrightness: number;
  glowOutwardOnly: boolean;
  glowGridPitch: number;
  glowGridCore: number;
  glowGridFade: number;
  glowGridDensity: number;
  edge: HUDEdge;
  offset: number;
  /** Native runtime display index; this is not a persistent hardware identity. */
  monitorIndex: number;
  logoSize: number;
  logoEdgeDistance: number;
  gapScale: number;
  showsLogos: boolean;
  simpleSize: number;
  simpleGapScale: number;
  ringWidth: number;
  showsRings: boolean;
  showsNumbers: boolean;
  separatesAgents: boolean;
  appearance: "dark" | "light" | "system";
  solidTransparency: number;
  hoverDelayMs: number;
  collapseDelayMs: number;
  requiresOptionToOpen: boolean;
  openImmediatelyAtTop: boolean;
  sessionList: boolean;
  quotaDetails: boolean;
  resetCountdown: boolean;
  tokenUsage: boolean;
  botLogos: boolean;
  panelWidth: number;
  layoutDensity: "comfortable" | "normal" | "compact";
  panelSections: ("blocks" | "tokens" | "sessions")[];
  panelOrder: string[];
  /** Native host measurements; the defaults are a non-notched display's stand-in. */
  hasNotch: boolean;
  notchWidth: number;
  notchHeight: number;
  animations: boolean;
}

export const defaultHUDConfig: HUDConfig = hudDefaults as HUDConfig;

export const HUD_METRICS = {
  duration: 0.4,
  ease: [0.2, 0.8, 0.2, 1] as [number, number, number, number],
  entryDistance: 5,
  collapsedFlare: 8,
  expandedFlare: 16,
  notchRadius: 12,
  fallbackRadius: 10,
  panelRadius: 26,
  agentPanelMinimum: 440,
  agentPanelInset: 100,
  defaultPanelHeight: 326,
  cardGap: 6,
  cardRadius: 22,
  bubbleWidth: 196,
  bubbleRadius: 14,
  dockHeight: 26,
  dockTextSize: 9,
  edgeMargin: 3,
  alertWidth: 470,
  alertWing: 112,
  alertPadding: 16,
} as const;

export interface HUDVendor {
  key: string;
  name: string;
  used: number | null;
  level: "normal" | "warning" | "critical" | "stale";
  working: number;
  waiting: number;
  balance: string | null;
  /** What an agent with no window and no balance shows instead: its last 24 hours at list prices, else their tokens (`recentLabel`). */
  usage: string | null;
  quotas: Snapshot["quotas"];
  sessions: Snapshot["sessions"];
}

/** Artwork identity is authoritative: Codex and ChatGPT use one OpenAI mark. */
export function vendorKey(source: string): string {
  return agentArtworkKey(source);
}

export { restingAgents, sourceID } from "./resting-agents.ts";

const vendorProjections = new WeakMap<
  Snapshot,
  Map<HUDMode, { value: HUDVendor[]; observedAt: number; expiresAt: number }>
>();

export function vendorsFromSnapshot(
  snapshot: Snapshot,
  mode: HUDMode,
): HUDVendor[] {
  const now = Date.now();
  let byMode = vendorProjections.get(snapshot);
  const cached = byMode?.get(mode);
  if (cached && now >= cached.observedAt && now <= cached.expiresAt)
    return cached.value;
  const { order: sources, expiresAt } = restingAgents(snapshot, mode, now);
  const value = sources.map((name) => {
    const key = vendorKey(name);
    const quotas = snapshot.quotas.filter((q) => vendorKey(q.vendor) === key);
    const measured = quotas.filter(
      (q) => q.usedPercent != null && q.status !== "stale",
    );
    const tightest = measured.reduce<(typeof measured)[number] | undefined>(
      (a, b) => (!a || b.usedPercent! > a.usedPercent! ? b : a),
      undefined,
    );
    const sessions = snapshot.sessions.filter(
      (s) => vendorKey(s.source) === key,
    );
    const balance = quotas.length
      ? undefined
      : snapshot.billing.find((b) => vendorKey(b.vendor) === key);
    const deepseekBalances =
      key === "deepseek" && !quotas.length
        ? snapshot.billing.filter((b) => vendorKey(b.vendor) === key)
        : [];
    return {
      key,
      name: agentVendorName(name),
      used: tightest?.usedPercent ?? null,
      // An agent billed by balance alone takes its balance's level.
      level:
        tightest?.status ??
        balanceLevel(deepseekBalances) ??
        balance?.level ??
        "stale",
      // In flight, as Open counts working: running or waiting on a request.
      working: sessions.filter(
        (s) => s.state === "running" || s.state === "waiting",
      ).length,
      waiting: snapshot.approvals.filter((a) => vendorKey(a.source) === key)
        .length,
      balance: deepseekBalances.length
        ? deepseekBalances
            .map((b) =>
              moneyAmount(b.amount, b.currency, snapshotLanguage(snapshot)),
            )
            .join(" · ")
        : balance
          ? moneyAmount(
              balance.amount,
              balance.currency,
              snapshotLanguage(snapshot),
            )
          : null,
      usage:
        quotas.length || balance
          ? null
          : recentLabel(recentUsage(snapshot, key), snapshotLanguage(snapshot)),
      quotas,
      sessions,
    };
  });
  if (!byMode) vendorProjections.set(snapshot, (byMode = new Map()));
  byMode.set(mode, { value, observedAt: now, expiresAt });
  return value;
}

/** A window's short name, as Core names it (the Swift `shortName`). */
export function quotaShortName(quota: {
  name: string;
  shortLabel?: string;
}): string {
  return quota.shortLabel || quota.name;
}

/** The number a Simple HUD writes beside a mark: its tightest window, else its balance, else its last 24 hours (vendors.rs `meter_label`). */
export function meterLabel(vendor: HUDVendor, display?: QuotaDisplay): string {
  return vendor.used == null
    ? (vendor.balance ?? vendor.usage ?? "--")
    : quotaPercentLabel(vendor.used, display);
}

export function statusColor(level: HUDVendor["level"]): string {
  return level === "critical"
    ? "#ff453a"
    : level === "warning"
      ? "#ffd23f"
      : level === "normal"
        ? "#3ddc84"
        : "#8e8e93";
}

/** HUDAgentOrder.swift: named marks in order; an unavailable list never empties a HUD. */
export function configuredVendors(
  snapshot: Snapshot,
  config: HUDConfig,
): HUDVendor[] {
  // Only the HUD's own list orders the marks; the panel's block order does not (Open ScreenAgents).
  const available = vendorsFromSnapshot(snapshot, config.mode).map((vendor) => {
      if (vendor.key !== "deepseek" || vendor.quotas.length) return vendor;
      const rows = snapshot.billing.filter(
        (row) => vendorKey(row.vendor) === vendor.key,
      );
      if (!rows.length) return vendor;
      const block = config.panelLayout.blocks?.[`api:${rows[0].vendor}`] ?? {};
      const visible = visibleBalances(rows, block);
      return {
        ...vendor,
        balance: visible.length
          ? visible
              .map((b) =>
                moneyAmount(b.amount, b.currency, snapshotLanguage(snapshot)),
              )
              .join(" · ")
          : null,
        level: balanceLevel(visible) ?? "stale",
        usage: null,
      };
    }),
    marks = (config.panelLayout.hud?.agents ?? []).map(vendorKey);
  if (!marks.some((mark) => available.some((vendor) => vendor.key === mark)))
    return available;
  return available
    .filter((vendor) => marks.includes(vendor.key))
    .sort((a, b) => marks.indexOf(a.key) - marks.indexOf(b.key));
}

function balanceLevel(
  rows: Snapshot["billing"],
): HUDVendor["level"] | undefined {
  if (!rows.length) return undefined;
  if (rows.some((row) => row.level === "critical")) return "critical";
  if (rows.some((row) => row.level === "warning")) return "warning";
  if (rows.some((row) => row.level === "normal")) return "normal";
  return "stale";
}

export function sourcesForMark(snapshot: Snapshot, mark: string): string[] {
  return snapshot.providers
    .filter((provider) => vendorKey(provider.id) === vendorKey(mark))
    .map((provider) => provider.id);
}

/** Account labels are display values; provider identity owns saved visibility. */
export function quotaAccountKey(row: Quota): string {
  return row.accountId ?? row.account ?? "unassigned";
}
export {
  inRowOrder,
  quotaRowHidden,
  quotaRowKey,
  quotaRowMatches,
  withRowShown,
} from "./quota-rows.ts";
