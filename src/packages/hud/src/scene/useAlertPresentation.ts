import { useEffect, useRef, type MutableRefObject } from "react";
import type { Approval, IslandEvent } from "@agent-hud/protocol";
import type { HUDConfig } from "../model";

/** How the HUD answers an island alert arriving or leaving: it opens or
 * stays collapsed as Open's ScreenHUD.present does, and closes after the
 * last one (ScreenHUD.closeAfterLastAlert). */
export function useAlertPresentation({
  config,
  open,
  pointerInside,
  typing,
  dragVisual,
  dragCandidate,
  suppressHover,
  enter,
  leave,
  forceOpen,
  collapse,
  setPinned,
  request,
  event,
  inUsagePanel,
}: {
  config: HUDConfig;
  open: boolean;
  pointerInside: boolean;
  typing: boolean;
  dragVisual: boolean;
  dragCandidate: MutableRefObject<{ armed: boolean; started: boolean } | null>;
  suppressHover: MutableRefObject<boolean>;
  enter: () => void;
  leave: () => void;
  forceOpen: () => void;
  collapse: () => void;
  setPinned: (value: boolean) => void;
  request?: Approval;
  event?: IslandEvent;
  inUsagePanel: boolean;
}) {
  const lastAlertId = useRef<string | null>(null);
  const lastAlertInUsagePanel = useRef(false);
  const activeAlertId = (request ?? event)?.id ?? null;

  useEffect(() => {
    if (activeAlertId !== null) {
      lastAlertInUsagePanel.current = Boolean(inUsagePanel);
    }
  }, [activeAlertId, inUsagePanel]);

  useEffect(() => {
    const previousId = lastAlertId.current;
    if (activeAlertId === previousId) return;
    lastAlertId.current = activeAlertId;
    if (activeAlertId !== null) {
      // ScreenHUD.present: cancel a pending hover timer so an alert arriving mid-delay
      // does not immediately pop open into the usage panel; if pointer is inside, restart delay.
      if (!open) {
        collapse();
        if (pointerInside && !suppressHover.current && !dragVisual) enter();
      }
      // On a separated dock, only permission requests force open; completion/quota/reset-credits
      // start collapsed as a compact pill until hovered or clicked.
      if (
        request &&
        !dragCandidate.current?.started &&
        config.mode !== "notch" &&
        config.separatesAgents
      ) {
        forceOpen();
      }
    } else if (previousId !== null) {
      if (lastAlertInUsagePanel.current) {
        lastAlertInUsagePanel.current = false;
        // A card inside the usage panel closes at once when the pointer is elsewhere.
        if (!pointerInside && !typing) collapse();
      } else {
        // ScreenHUD.closeAfterLastAlert: when the last alert leaves, collapse,
        // and a pointer still on the HUD does not reopen the usage panel
        // until it leaves.
        if (pointerInside) suppressHover.current = true;
        collapse();
        setPinned(false);
      }
    }
  }, [
    activeAlertId,
    Boolean(request),
    config.mode,
    config.separatesAgents,
    open,
    pointerInside,
    typing,
    dragVisual,
    collapse,
    enter,
    leave,
    forceOpen,
    dragCandidate,
    suppressHover,
    setPinned,
  ]);
}
