/* The windows a Simple mark draws round and beside its logo: those of one
 * account, as the panel shows them. */
import type { PanelLayout, Quota } from "@agent-hud/protocol";
import { blockRows, quotaAccountKey } from "../quota-rows.ts";
import { MAX_USAGE_ITEMS } from "./arcs.ts";

/** One item a ring or bar draws: its share from 0 to 100, and the colour of its level. */
export interface RingItem {
  percent: number | null;
  color: string;
}

/** The windows a Simple mark draws, in the panel's order: those the panel
 * shows of the account that holds the agent's tightest window (its last
 * reading's, while every window waits on a failed read), at most four. An
 * agent with no window, such as one billed by balance alone, draws none. */
export function markWindows(
  vendor: { quotas: readonly Quota[] },
  layout?: PanelLayout,
  now = Date.now(),
): Quota[] {
  const shown = vendor.quotas.filter(
    (row) => blockRows([row], layout?.blocks?.[row.vendor]).length,
  );
  if (!shown.length) return [];
  const live = shown.filter(
    (q) => q.usedPercent != null && q.status !== "stale",
  );
  const pool = live.length
    ? live
    : shown.filter((q) => q.usedPercent != null && !resetPassed(q, now));
  const tightest = pool.reduce<Quota | undefined>(
    (a, b) => (!a || b.usedPercent! > a.usedPercent! ? b : a),
    undefined,
  );
  const account = quotaAccountKey(tightest ?? shown[0]);
  const rows = shown.filter((q) => quotaAccountKey(q) === account);
  return blockRows(rows, layout?.blocks?.[rows[0].vendor]).slice(
    0,
    MAX_USAGE_ITEMS,
  );
}

export function resetPassed(q: Quota, now: number): boolean {
  const reset = q.resetAt ? Date.parse(q.resetAt) : NaN;
  return reset <= now;
}

/** The track under a Simple mark's arcs and bars. */
export const markTrack = (light: boolean) =>
  light ? "#0000000f" : "#ffffff1a";
