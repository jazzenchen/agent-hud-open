import { useEffect } from "react";
import type { HUDConfig, HUDVendor } from "../model";

/** Responds to host open/close panel commands (`panelCommand.sequence`). */
export function usePanelCommand({
  panelCommand,
  config,
  vendors,
  setHovered,
  setPinned,
  forceOpen,
  collapse,
}: {
  panelCommand?: { open: boolean; sequence: number };
  config: Pick<HUDConfig, "mode" | "separatesAgents">;
  vendors: readonly HUDVendor[];
  setHovered: (value: string | null) => void;
  setPinned: (value: boolean) => void;
  forceOpen: () => void;
  collapse: () => void;
}) {
  useEffect(() => {
    if (!panelCommand) return;
    if (!panelCommand.open) {
      setPinned(false);
      collapse();
      return;
    }
    // A docked HUD that separates agents opens on its first agent, as a hold does.
    if (
      config.mode !== "notch" &&
      config.separatesAgents &&
      vendors.length > 0
    ) {
      setHovered(vendors[0].key);
      setPinned(true);
    }
    forceOpen();
  }, [panelCommand?.sequence]);
}
