/* Which agents a screen's HUD shows, as the layout's `hud.agents` lists them
 * (resting-agents.ts `layoutMarks`, vendors.rs `configured`): none listed, or
 * none of those here now, shows every agent. */

/** The agents shown out of those the HUD has now. */
export function shownAgents(available: string[], saved: string[]): string[] {
  const shown = available.filter((key) => saved.includes(key));
  return shown.length ? shown : [...available];
}

/** The list after one agent is switched on or off. One always stays on;
 * all of them on is saved as none listed, so an agent added later shows too;
 * listed agents the HUD does not have now keep their place. */
export function toggledAgents(
  available: string[],
  saved: string[],
  key: string,
): string[] {
  const shown = shownAgents(available, saved);
  const next = shown.includes(key)
    ? shown.filter((item) => item !== key)
    : [...shown, key];
  if (!next.length) return saved;
  if (available.every((item) => next.includes(item))) return [];
  return [...saved.filter((item) => !available.includes(item)), ...next];
}
