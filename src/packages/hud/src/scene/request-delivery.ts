import { useMemo } from "react";
import type { Approval, RequestDelivery, Snapshot } from "@agent-hud/protocol";
import { vendorKey } from "../model";

/** The newest answer from the last five minutes whose delivery to its
 * client is not confirmed: the request's on show, or else the focused
 * agent's. */
export function useRequestDelivery(
  snapshot: Snapshot,
  request: Approval | undefined,
  focusedKey: string | undefined,
) {
  return useMemo(() => {
    const now = Date.parse(snapshot.generatedAt);
    const source = request?.source ?? focusedKey;
    let latest: RequestDelivery | undefined;
    let latestAt = -Infinity;
    for (const entry of snapshot.requestDeliveries ?? []) {
      const at = Date.parse(entry.at);
      if (entry.state === "confirmed" || now - at > 300_000) continue;
      if (source && vendorKey(entry.source) !== vendorKey(source)) continue;
      if (request?.sessionId && entry.sessionId !== request.sessionId) continue;
      if (at > latestAt) {
        latest = entry;
        latestAt = at;
      }
    }
    return latest;
  }, [
    snapshot.generatedAt,
    snapshot.requestDeliveries,
    request?.source,
    request?.sessionId,
    focusedKey,
  ]);
}
