import type { Quota, Snapshot } from "@agent-hud/protocol";
import { agentArtworkKey } from "./agent-artwork";
import { leadingPointsPercent } from "./panel/points-detail.ts";

/** A points-detail percent drawn as a quota ring. It is already the remaining share. */
export function isPointsShareQuota(quota: { id: string }): boolean {
  return quota.id.endsWith(":points-share");
}

/** The ring for an agent whose only reading is a points balance, not a quota window. */
export function pointsShareQuota(
  snapshot: Snapshot,
  key: string,
): Quota | null {
  const percent = snapshot.billing
    .filter((row) => agentArtworkKey(row.vendor) === key)
    .map((row) => leadingPointsPercent(row.pointsDetail?.groups))
    .find((share) => share != null);
  if (percent == null) return null;
  return {
    id: `${key}:points-share`,
    vendor: key,
    name: "剩余额度",
    usedPercent: percent,
    resetAt: null,
    status: "normal",
    account: null,
  };
}
