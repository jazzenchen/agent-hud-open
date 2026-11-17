import { useEffect } from "react";
import type { HUDConfig, HUDVendor } from "../model";

/** A host's order to open or close the usage panel, as a demo script's panel
 * step gives (Open DemoScriptRunner: island forceOpen/forceCollapse). Each
 * new `sequence` is one order. A docked HUD that separates agents opens on
 * its first agent, as a hold does. */
export function usePanelCommand(
  command: { open: boolean; sequence: number } | undefined,
  scene: {
    config: HUDConfig;
    vendors: HUDVendor[];
    setHovered: (key: string | null) => void;
    setPinned: (pinned: boolean) => void;
    forceOpen: () => void;
    collapse: () => void;
  },
) {
  useEffect(() => {
    if (!command) return;
    const { config, vendors } = scene;
    if (!command.open) {
      scene.setPinned(false);
      scene.collapse();
      return;
    }
    if (
      config.mode !== "notch" &&
      config.separatesAgents &&
      vendors.length > 0
    ) {
      scene.setHovered(vendors[0].key);
      scene.setPinned(true);
    }
    scene.forceOpen();
  }, [command?.sequence]);
}
