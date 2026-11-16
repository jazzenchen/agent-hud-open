import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { currentMonitor, getCurrentWindow } from "@tauri-apps/api/window";
import type { Snapshot } from "@agent-hud/protocol";
import { getScreenHUDConfig, type HUDConfigStore } from "./hud-config-store";
import type { NativeHUDMonitor } from "./use-hud-geometry";

/** The window's look: the Appearance setting, the system's while that says
 * System, or with Match screen on, the look of the screen the window is on.
 * The document is marked with it. */
export function useWindowTheme(
  snapshot: Snapshot | null,
  store: HUDConfigStore,
  monitors: NativeHUDMonitor[],
  surface: boolean,
) {
  const [initialAppearance] = useState(
    () => document.documentElement.dataset.initialAppearance,
  );
  const [systemLight, setSystemLight] = useState(
    () => matchMedia("(prefers-color-scheme: light)").matches,
  );
  useEffect(() => {
    const media = matchMedia("(prefers-color-scheme: light)");
    const change = () => setSystemLight(media.matches);
    media.addEventListener("change", change);
    return () => media.removeEventListener("change", change);
  }, []);
  const [windowScreen, setWindowScreen] = useState<string | null>(null);
  useEffect(() => {
    if (!isTauri() || surface || !snapshot?.settings.appearanceFollowsScreen)
      return;
    let disposed = false;
    const window = getCurrentWindow();
    const read = () =>
      void currentMonitor().then((monitor) => {
        if (disposed || !monitor) return;
        const match = monitors.find(
          (m) =>
            m.bounds.x === monitor.position.x / monitor.scaleFactor &&
            m.bounds.y === monitor.position.y / monitor.scaleFactor,
        );
        setWindowScreen(match?.screenId ?? null);
      });
    read();
    const moved = window.onMoved(read);
    return () => {
      disposed = true;
      void moved.then((unlisten) => unlisten());
    };
  }, [surface, monitors, snapshot?.settings.appearanceFollowsScreen]);
  const savedAppearance = snapshot?.settings.appearance ?? initialAppearance;
  const fallbackAppearance =
    savedAppearance === "light" || savedAppearance === "dark"
      ? savedAppearance
      : "system";
  const appearance =
    snapshot?.settings.appearanceFollowsScreen && windowScreen
      ? getScreenHUDConfig(store, {
          id: windowScreen,
          monitorIndex: 0,
          hasNotch: false,
        }).appearance
      : fallbackAppearance;
  const theme =
    appearance === "system" ? (systemLight ? "light" : "dark") : appearance;
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.classList.toggle("dark", theme === "dark");
    if (isTauri() && !surface)
      void invoke("window_appearance", { theme }).catch((error) =>
        console.error("Agent HUD window appearance:", error),
      );
  }, [theme, surface]);
  return theme;
}
