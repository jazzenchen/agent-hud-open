import { useEffect, useRef } from "react";
import { toast } from "sonner";
import type { Snapshot } from "@agent-hud/protocol";
import { snapshotLanguage, translator } from "@agent-hud/ui/format";

/** Shows a client's failed or unconfirmed answer to a request as a toast,
 * once each and only while `enabled`; an answer older than five minutes
 * stays quiet. */
export function useRequestDeliveryToasts(
  snapshot: Snapshot | null,
  enabled: boolean,
) {
  const seenRequestDeliveries = useRef(new Set<string>());
  useEffect(() => {
    if (!enabled) return;
    for (const delivery of snapshot?.requestDeliveries ?? []) {
      const key = `${delivery.id}:${delivery.state}`;
      if (seenRequestDeliveries.current.has(key)) continue;
      seenRequestDeliveries.current.add(key);
      if (
        delivery.state === "confirmed" ||
        Date.now() - Date.parse(delivery.at) > 300_000
      )
        continue;
      // App calls this outside its FormatProvider, so the language is read
      // from the snapshot here.
      const t = translator(snapshotLanguage(snapshot));
      const title =
        delivery.state === "uncertain"
          ? t("Client response unconfirmed")
          : t("Client response failed");
      const description =
        delivery.reason ??
        t("Check the original client before taking another action.");
      if (delivery.state === "uncertain") toast.warning(title, { description });
      else toast.error(title, { description });
    }
  }, [enabled, snapshot?.requestDeliveries]);
}
