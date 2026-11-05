import type { QuotaHistoryWindow } from "@agent-hud/protocol";

/** One agent's windows drawn in one chart; an agent signed in with several
 * accounts has one chart per account. */
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

/** A window's line colour by its place in its chart: the accent first, then
 * hues far from it and from each other. */
export function windowColor(index: number, light: boolean): string {
  const colors = light
    ? ["#007aff", "#e8590c", "#0ca678", "#9c36b5", "#d6336c", "#5c940d"]
    : ["#0a84ff", "#ff9f0a", "#30d158", "#bf5af2", "#ff6482", "#a3e635"];
  return colors[index % colors.length];
}

/** Each window's reading nearest to `at`, where one lies within `reach`
 * (epoch ms) of it; null for a window with none there, so no reading is
 * offered across a gap. */
export function readingsAt(
  windows: Pick<QuotaHistoryWindow, "segments">[],
  at: number,
  reach: number,
): ([number, number] | null)[] {
  return windows.map((window) => {
    let nearest: [number, number] | null = null;
    for (const run of window.segments)
      for (const point of run)
        if (!nearest || Math.abs(point[0] - at) < Math.abs(nearest[0] - at))
          nearest = point;
    return nearest && Math.abs(nearest[0] - at) <= reach ? nearest : null;
  });
}
