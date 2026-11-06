import { agentAccent } from "../StatsTheme";

const PALETTE = [
  "#c084fc",
  "#60a5fa",
  "#2dd4bf",
  "#fb923c",
  "#f472b6",
  "#a3e635",
];

/** An agent's colour on the overview: its own, as its card wears it, else
 * one of the HUD's agent hues by its name. */
export function agentColor(id: string): string {
  const own = agentAccent(id);
  if (own) return own;
  let hash = 0;
  for (const char of id) hash = ((hash << 5) - hash + char.charCodeAt(0)) | 0;
  return PALETTE[(hash >>> 0) % PALETTE.length];
}
