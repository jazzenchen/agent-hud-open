import { useEffect, useState } from "react";
import { storageKeys } from "./storage-keys";
import type { NativeHUDMonitor } from "./use-hud-geometry";

const selectedScreenKey = storageKeys.selectedScreen;

/** The screen a window shows: a HUD window's own, else the one last picked
 * in Settings, the primary screen or the first. Settings keeps the screen it
 * opens on as the pick until another is picked. */
export function useSelectedScreen(
  surfaceScreenId: string | undefined,
  monitors: NativeHUDMonitor[],
  settings: boolean,
) {
  const [preferredScreenId, setPreferredScreenId] = useState(
    () => localStorage.getItem(selectedScreenKey) || "",
  );
  const selectedId =
    surfaceScreenId !== undefined
      ? surfaceScreenId
      : (monitors.find((monitor) => monitor.screenId === preferredScreenId)
          ?.screenId ??
        monitors.find((monitor) => monitor.primary)?.screenId ??
        monitors[0]?.screenId ??
        "");
  function selectScreen(id: string) {
    setPreferredScreenId(id);
    localStorage.setItem(selectedScreenKey, id);
  }
  useEffect(() => {
    if (settings && selectedId && !preferredScreenId) selectScreen(selectedId);
  }, [settings, selectedId, preferredScreenId]);
  return { selectedId, selectScreen };
}
