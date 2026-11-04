import { useCallback, useEffect, useRef, useState } from "react";
import type { PanelServices } from "../PanelContext";
import type { useHoverMachine } from "../hover";
import type { HUDConfig, HUDVendor } from "../model";

/** An agent's edit hold: the panel stays open while an agent edits it, until
 * the hold runs out or the person takes over. */
export function useEditHold({
  hover,
  panelServices,
  config,
  vendors,
  hovered,
  setHovered,
  setPinned,
}: {
  hover: ReturnType<typeof useHoverMachine>;
  panelServices: PanelServices | null;
  config: HUDConfig;
  vendors: HUDVendor[];
  hovered: string | null;
  setHovered: (key: string | null) => void;
  setPinned: (pinned: boolean) => void;
}) {
  const heldUntil = panelServices?.draft?.heldUntil ?? 0;
  const [holdActive, setHoldActive] = useState(false);
  // An agent's edit hold ends when the person takes over (Open
  // ScreenHUD+Hold): Escape, the pointer coming and going, Settings or
  // Statistics, or a drag. The panel then follows the pointer again.
  const enteredDuringHold = useRef(false);
  const endHold = useCallback(() => {
    if (!holdActive) return;
    setHoldActive(false);
    enteredDuringHold.current = false;
    panelServices?.draft?.release();
    hover.settle();
  }, [holdActive, panelServices, hover.settle]);
  const held = useRef(false);
  useEffect(() => {
    const remaining = heldUntil - Date.now();
    const wasHeld = held.current;
    held.current = remaining > 0;
    setHoldActive(remaining > 0);
    enteredDuringHold.current = false;
    if (remaining <= 0) {
      // Only a hold's end hands the panel back to the pointer. Running on
      // every new snapshot, this closed a panel opened any other way (a
      // demo's panel step, the keyboard) within a second.
      if (wasHeld) hover.settle();
      return;
    }
    if (
      config.mode !== "notch" &&
      config.separatesAgents &&
      !hovered &&
      vendors.length > 0
    ) {
      setHovered(vendors[0].key);
      setPinned(true);
    }
    hover.forceOpen();
    const timer = setTimeout(() => {
      held.current = false;
      setHoldActive(false);
      hover.settle();
    }, remaining);
    return () => clearTimeout(timer);
  }, [heldUntil]);
  const enter = useCallback(() => {
    if (holdActive) enteredDuringHold.current = true;
    hover.enter();
  }, [holdActive, hover.enter]);
  const leave = useCallback(() => {
    if (!holdActive) hover.leave();
    else if (enteredDuringHold.current) {
      hover.inside.current = false;
      endHold();
    } else hover.inside.current = false;
  }, [holdActive, hover.leave, endHold]);
  return { holdActive, endHold, enter, leave };
}
