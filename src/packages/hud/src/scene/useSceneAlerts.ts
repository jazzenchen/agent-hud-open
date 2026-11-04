import { useEffect, useMemo, useRef } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { glowAlert } from "../glow-alert";
import { useIslandNews } from "../IslandNews";
import { vendorKey, type HUDConfig, type HUDVendor } from "../model";

/** The island alert on show (a request or an event) and those queued behind
 * it, whether it sits inside the open usage panel, and the rim's flash for it. */
export function useSceneAlerts({
  snapshot,
  config,
  vendors,
  held,
  silenced,
  underPointer,
  open,
}: {
  snapshot: Snapshot;
  config: HUDConfig;
  vendors: HUDVendor[];
  /** The pointer or the keyboard is on the HUD: an event stays. */
  held: boolean;
  silenced: boolean;
  underPointer: boolean;
  open: boolean;
}) {
  const requests = [...snapshot.approvals].sort(
    (a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt),
  );
  const usagePanelOpenRef = useRef(false);
  const allowsInlineUsageAlert = !(
    config.mode !== "notch" && config.separatesAgents
  );
  const news = useIslandNews(
    snapshot,
    held,
    requests,
    vendors.map((v) => v.key),
    silenced,
    underPointer,
    open && allowsInlineUsageAlert,
  );
  const event = news.event;
  const request = news.request;
  // The rim flashes once for each island alert, over its sender's colours.
  const glowFlash = useMemo(
    () =>
      glowAlert(
        request
          ? {
              id: request.id,
              kind: "request",
              vendor: vendorKey(request.source),
            }
          : event
            ? {
                id: event.id,
                kind: event.kind,
                vendor: vendorKey(event.source),
              }
            : null,
        vendors
          .filter((vendor) => vendor.used != null)
          .map((vendor) => vendor.key),
      ),
    [request?.id, event?.id, vendors],
  );
  useEffect(() => {
    if (open && !request && !event) {
      usagePanelOpenRef.current = true;
    } else if (!open) {
      usagePanelOpenRef.current = false;
    }
  }, [open, request, event]);
  const inUsagePanel = Boolean(
    open &&
    !request &&
    event &&
    allowsInlineUsageAlert &&
    (news.inUsagePanel || usagePanelOpenRef.current),
  );
  return { requests, news, request, event, glowFlash, inUsagePanel };
}
