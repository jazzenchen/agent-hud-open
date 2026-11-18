import { HUDArtwork } from "@agent-hud/hud/HUDArtwork";
import { useLightTheme } from "./agent-plugins/use-status";

/** An agent's logo in Settings: its one-colour marks follow the window's
 * theme, dark on the light theme and white on the dark one. */
export function AgentLogo({ source, size }: { source: string; size?: number }) {
  return <HUDArtwork source={source} size={size} light={useLightTheme()} />;
}
