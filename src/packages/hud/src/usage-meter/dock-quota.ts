import type { Quota } from "@agent-hud/protocol";

export const DOCK_QUOTA_NORMAL_MS = 24_000;
export const DOCK_QUOTA_WARNING_MS = 6_000;
export const DOCK_QUOTA_THRESHOLDS = [0, 5, 10, 15, 20, 30] as const;
export type DockQuotaThreshold = (typeof DOCK_QUOTA_THRESHOLDS)[number];

type MeasuredWindow = Quota & {
  usedPercent: number;
  windowDurationSeconds: number;
};

export function dockQuotaThreshold(value: unknown): DockQuotaThreshold {
  return DOCK_QUOTA_THRESHOLDS.find((option) => option === value) ?? 10;
}

/** Unknown durations cannot override a known short window. */
export function dockQuotaWindows(quotas: readonly Quota[], threshold: unknown) {
  const known = quotas.filter(
    (q): q is MeasuredWindow =>
      q.status !== "stale" &&
      q.usedPercent != null &&
      Number.isFinite(q.usedPercent) &&
      q.windowDurationSeconds != null &&
      Number.isFinite(q.windowDurationSeconds) &&
      q.windowDurationSeconds > 0,
  );
  let primary: MeasuredWindow | undefined;
  for (const q of known) {
    if (
      !primary ||
      q.windowDurationSeconds < primary.windowDurationSeconds ||
      (q.windowDurationSeconds === primary.windowDurationSeconds &&
        q.usedPercent > primary.usedPercent)
    )
      primary = q;
  }
  let warning: MeasuredWindow | undefined;
  const cutoff = dockQuotaThreshold(threshold);
  if (cutoff === 0) return { primary, warning, warningWindows: [] };
  const warningWindows: MeasuredWindow[] = [];
  for (const q of known) {
    if (
      primary &&
      q.windowDurationSeconds > primary.windowDurationSeconds &&
      q.usedPercent > 100 - cutoff
    ) {
      warningWindows.push(q);
      if (!warning || q.usedPercent > warning.usedPercent) warning = q;
    }
  }
  return { primary, warning, warningWindows };
}

export function quotaAlertCycle(q: Pick<Quota, "id" | "resetAt">): string {
  return q.resetAt ?? "without-reset";
}

export function takeQuotaAlert(
  seen: Pick<Storage, "getItem" | "setItem">,
  q: Pick<Quota, "id" | "resetAt">,
): boolean {
  const key = `agent-hud:dock-quota-alert:${q.id}`;
  const cycle = quotaAlertCycle(q);
  if (seen.getItem(key) === cycle) return false;
  seen.setItem(key, cycle);
  return true;
}
