import { useEffect, useMemo, useRef } from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { glowAlert } from "../glow-alert";
import { useIslandNews } from "../IslandNews";
import { vendorKey, type HUDConfig, type HUDVendor } from "../model";
import type { EndSide } from "../ends/end-geometry";
import type { ParkedCard } from "../ends/end-queue";

/** The island alert on show (a request or an event) and those queued behind
 * it, whether it sits inside the open usage panel, and the rim's flash for it.
 * An end whose panel is open shows what waits there instead; `endCounts` is
 * how many each end shows, leaving out cards still flying to it. */
export function useSceneAlerts({
  snapshot,
  config,
  vendors,
  held,
  silenced,
  underPointer,
  open,
  ends,
}: {
  snapshot: Snapshot;
  config: HUDConfig;
  vendors: HUDVendor[];
  /** The pointer or the keyboard is on the HUD: an event stays. */
  held: boolean;
  silenced: boolean;
  underPointer: boolean;
  open: boolean;
  ends: {
    /** The end the pointer is on. */
    hovered: EndSide | null;
    onPark: (card: ParkedCard, source: string) => void;
    arriving: ReadonlySet<string>;
  };
}) {
  const allRequests = [...snapshot.approvals].sort(
    (a, b) => Date.parse(a.createdAt) - Date.parse(b.createdAt),
  );
  const usagePanelOpenRef = useRef(false);
  const allowsInlineUsageAlert = !(
    config.mode !== "notch" && config.separatesAgents
  );
  const news = useIslandNews(
    snapshot,
    held,
    allRequests,
    vendors.map((v) => v.key),
    silenced,
    underPointer,
    open && allowsInlineUsageAlert,
    { hovered: open ? ends.hovered : null, onPark: ends.onPark },
  );
  const event = news.event;
  const request = news.request;
  const parked = news.ends;
  // The requests a card lists: the head's own when it is open, otherwise
  // those not parked there yet.
  const requests =
    parked.view === "head"
      ? parked.head
      : allRequests.filter((r) => !parked.head.some((h) => h.id === r.id));
  const endCounts = {
    head: parked.head.filter((r) => !ends.arriving.has(r.id)).length,
    tail: parked.tail.filter((m) => !ends.arriving.has(m.id)).length,
  };
  // The rim flashes once for each card that comes out, over its sender's colours.
  const { request: freshRequest, event: freshEvent } = news.fresh;
  const glowFlash = useMemo(
    () =>
      glowAlert(
        freshRequest
          ? {
              id: freshRequest.id,
              kind: "request",
              vendor: vendorKey(freshRequest.source),
            }
          : freshEvent
            ? {
                id: freshEvent.id,
                kind: freshEvent.kind,
                vendor: vendorKey(freshEvent.source),
              }
            : null,
        vendors
          .filter((vendor) => vendor.used != null)
          .map((vendor) => vendor.key),
      ),
    [freshRequest?.id, freshEvent?.id, vendors],
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
    !parked.view &&
    !request &&
    event &&
    allowsInlineUsageAlert &&
    (news.inUsagePanel || usagePanelOpenRef.current),
  );
  return {
    requests,
    news,
    request,
    event,
    glowFlash,
    inUsagePanel,
    endCounts,
  };
}
