/**
 * Every localStorage key the app's windows use. Builds before the app took
 * its identifiers kept them under "agenthud.next."; loading this module
 * carries those values over once.
 */
export const storageKeys = {
  selectedScreen: "agenthud.selected-screen",
  highlightProvider: "agenthud.highlight-provider",
  hiddenAgentCards: "agenthud.hidden-agent-cards",
  hudConfig: "agenthud.hud-config.v3",
  hudConfigV2: "agenthud.hud-config.v2",
  hudConfigV1: "agenthud.hud-config",
} as const;

const prefix = "agenthud.";
const earlierPrefix = "agenthud.next.";

/** Moves each value an earlier build kept to its key here. A value already
 * under the new key stays. */
export function adoptEarlierKeys(storage: Storage): void {
  for (const key of Object.values(storageKeys)) {
    const earlier = earlierPrefix + key.slice(prefix.length);
    const value = storage.getItem(earlier);
    if (value === null) continue;
    if (storage.getItem(key) === null) storage.setItem(key, value);
    storage.removeItem(earlier);
  }
}

try {
  adoptEarlierKeys(localStorage);
} catch {}
