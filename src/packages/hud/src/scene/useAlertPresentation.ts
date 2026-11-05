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
  ends,
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
  /** Whether the alert on show is an end's, and which end the pointer is on
   * (none once it moved onto the dock). */
  ends?: { showing: boolean; hovered: string | null };
}) {
  const lastAlertId = useRef<string | null>(null);
  const lastAlertInUsagePanel = useRef(false);
  const lastFromEnd = useRef(false);
  const activeAlertId = (request ?? event)?.id ?? null;

  useEffect(() => {
    if (activeAlertId !== null) {
      lastAlertInUsagePanel.current = Boolean(inUsagePanel);
      lastFromEnd.current = Boolean(ends?.showing);
    }
  }, [activeAlertId, inUsagePanel, ends?.showing]);

  useEffect(() => {
    const previousId = lastAlertId.current;
    if (activeAlertId === previousId) return;
    lastAlertId.current = activeAlertId;
    if (activeAlertId !== null) {
      // ScreenHUD.present: cancel a pending hover timer so an alert arriving mid-delay
      // does not immediately pop open into the usage panel; if pointer is inside, restart delay.
      // A card after one that was forced open, with the pointer away from
      // the HUD, starts small as well.
      const forced = open && previousId !== null && !pointerInside && !typing;
      if (!open || forced) {
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
      if (lastFromEnd.current && pointerInside && !ends?.hovered) {
        // From an end's panel onto the dock: the panel stays open for it.
        lastFromEnd.current = false;
      } else if (lastAlertInUsagePanel.current) {
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
