/** The colours a mark draws its level and its number in. */
import type { HUDConfig, HUDVendor } from "./model.ts";

export function statusColor(level: HUDVendor["level"]): string {
  return level === "critical"
    ? "#ff453a"
    : level === "warning"
      ? "#ffd23f"
      : level === "normal"
        ? "#3ddc84"
        : "#8e8e93";
}

/** The number's own ink, shared by the resting HUD and the settings preview. */
export function simpleNumberColor(
  config: Pick<HUDConfig, "greyNumbers" | "greyNumberOpacity">,
  light: boolean,
  levelColor: string,
): string {
  if (!config.greyNumbers) return levelColor;
  const shade = light ? 64 : 212;
  return `rgba(${shade}, ${shade}, ${shade}, ${config.greyNumberOpacity})`;
}
