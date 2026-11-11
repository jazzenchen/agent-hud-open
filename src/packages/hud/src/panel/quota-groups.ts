import type { QuotaHistoryWindow } from "@agent-hud/protocol";

/** One agent's windows drawn together; an agent signed in with several
 * accounts has one group per account. */
export interface QuotaGroup {
  key: string;
  vendor: string;
  /** The account's name, given only where the vendor has several. */
  account: string | null;
  windows: QuotaHistoryWindow[];
}

/** The windows grouped by agent, and by account where an agent has several,
 * each group where its first window stands and its windows in their order. */
export function quotaGroups(windows: QuotaHistoryWindow[]): QuotaGroup[] {
  const accounts = new Map<string, Set<string>>();
  for (const window of windows)
    accounts.set(
      window.vendor,
      (accounts.get(window.vendor) ?? new Set()).add(window.accountId ?? ""),
    );
  const groups = new Map<string, QuotaGroup>();
  for (const window of windows) {
    const several = (accounts.get(window.vendor)?.size ?? 0) > 1;
    const key = several
      ? `${window.vendor}|${window.accountId ?? ""}`
      : window.vendor;
    const group = groups.get(key) ?? {
      key,
      vendor: window.vendor,
      account: several ? (window.account ?? null) : null,
      windows: [],
    };
    group.windows.push(window);
    groups.set(key, group);
  }
  return [...groups.values()];
}

/** A cycle's level by what it used, at the account readers' thresholds:
 * warning from 70%, critical from 90%. */
export function cycleLevel(used: number): "normal" | "warning" | "critical" {
  return used >= 90 ? "critical" : used >= 70 ? "warning" : "normal";
}

/** The stretch a cycle chart draws: the range up to `to`, then a little past
 * it so the running cycles show up to their resets. */
export function cycleSpan(to: number, days: number) {
  const length = days * 86_400_000;
  return { from: to - length, until: to + length * 0.18 };
}
