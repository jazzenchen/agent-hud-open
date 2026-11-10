/** A dock selection is optional: an absent/unavailable list means all known marks.
 * Keep at least one available entry so Settings remains reachable from the dock. */
export function selectedAgentKeys(
  available: string[],
  saved: string[] = [],
): string[] {
  const selected = available.filter((key) => saved.includes(key));
  return selected.length ? selected : [...available];
}

export function toggleAgentKey(
  available: string[],
  saved: string[],
  key: string,
): string[] {
  const selected = selectedAgentKeys(available, saved);
  if (!available.includes(key)) return selected;
  if (!selected.includes(key)) return [...selected, key];
  return selected.length > 1
    ? selected.filter((item) => item !== key)
    : selected;
}
