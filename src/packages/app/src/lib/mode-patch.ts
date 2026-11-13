import type { HUDConfig } from "@agent-hud/hud/model";

/** A screen's HUD switched to `mode`. A notch HUD still at the top centre of
 * a notched screen goes to the top left as Simple, out of the notch's way. */
export function modePatch(
  config: HUDConfig,
  mode: HUDConfig["mode"],
): Partial<HUDConfig> {
  return {
    mode,
    ...(config.mode === "notch" &&
    mode !== "notch" &&
    config.hasNotch &&
    config.edge === "top" &&
    config.offset === 0.5
      ? { offset: 0 }
      : {}),
  };
}
