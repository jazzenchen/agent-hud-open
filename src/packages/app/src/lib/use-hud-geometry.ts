import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export interface NativeRect {
  x: number;
  y: number;
  width: number;
  height: number;
}
/** Read-only measurements. Screen coordinates are never saved as presentation preferences. */
export interface NativeHUDGeometry {
  screenId: string;
  monitorIndex: number;
  scaleFactor: number;
  coordinateSpace:
    "global-logical-top-left" | "monitor-scaled-physical-top-left";
  screen: NativeRect;
  menuBarHeight: number;
  hasNotch: boolean;
  notch: NativeRect | null;
  canvas: NativeRect;
  notchInViewport: NativeRect | null;
  placementConstrained: boolean;
  followsDepth?: boolean;
}
export interface NativeHUDMonitor {
  screenId: string;
  persistentId: string | null;
  runtimeId: string;
  windowLabel: string;
  index: number;
  name: string | null;
  primary: boolean;
  bounds: NativeRect;
  scaleFactor: number;
  notch: NativeRect | null;
}
const preview: NativeHUDMonitor = {
  screenId: "preview",
  persistentId: null,
  runtimeId: "preview",
  windowLabel: "preview",
  index: 0,
  name: "Preview",
  primary: true,
  bounds: { x: 0, y: 0, width: 1440, height: 900 },
  scaleFactor: 1,
  notch: null,
};

/** The connected screens and their HUD geometry. The app measures them and
 * tells every active window; `ownScreenId` limits a HUD to its own screen. */
export function useHUDGeometry(ownScreenId?: string, enabled = true) {
  const [geometries, setGeometries] = useState<
    Record<string, NativeHUDGeometry>
  >({});
  const [monitors, setMonitors] = useState<NativeHUDMonitor[]>(() =>
    isTauri() ? [] : [preview],
  );
  useEffect(() => {
    if (!enabled || !isTauri()) return;
    let disposed = false;
    let signature = "";
    const revisions = new Map<string, number>();
    const seenGeometry = new Set<string>();
    const applyMonitors = (value: NativeHUDMonitor[]) => {
      if (disposed) return;
      const next = JSON.stringify(value);
      if (next === signature) return;
      signature = next;
      setMonitors(value);
      const connected = new Set(value.map((monitor) => monitor.screenId));
      for (const id of seenGeometry)
        if (!connected.has(id)) seenGeometry.delete(id);
      setGeometries((previous) =>
        Object.fromEntries(
          Object.entries(previous).filter(([id]) => connected.has(id)),
        ),
      );
      const targets =
        ownScreenId !== undefined
          ? value.filter((monitor) => monitor.screenId === ownScreenId)
          : value;
      for (const monitor of targets) {
        if (seenGeometry.has(monitor.screenId)) continue;
        const before = revisions.get(monitor.screenId) ?? 0;
        void invoke<NativeHUDGeometry | null>("hud_geometry", {
          screenId: monitor.screenId,
        })
          .then((geometry) => {
            if (
              disposed ||
              !geometry ||
              (revisions.get(monitor.screenId) ?? 0) !== before
            )
              return;
            seenGeometry.add(geometry.screenId);
            setGeometries((previous) => ({
              ...previous,
              [geometry.screenId]: geometry,
            }));
          })
          .catch((error) =>
            console.error("HUD geometry measurement failed", error),
          );
      }
    };
    const here = getCurrentWebviewWindow();
    const geometrySubscription = here.listen<NativeHUDGeometry>(
      "hud:geometry",
      (event) => {
        if (disposed) return;
        const value = event.payload;
        if (ownScreenId !== undefined && value.screenId !== ownScreenId) return;
        revisions.set(value.screenId, (revisions.get(value.screenId) ?? 0) + 1);
        seenGeometry.add(value.screenId);
        setGeometries((previous) => ({ ...previous, [value.screenId]: value }));
      },
    );
    const monitorSubscription = here.listen<NativeHUDMonitor[]>(
      "hud:monitors",
      (event) => applyMonitors(event.payload),
    );
    void Promise.all([geometrySubscription, monitorSubscription])
      .then(() =>
        disposed ? null : invoke<NativeHUDMonitor[]>("hud_monitors"),
      )
      .then((value) => {
        if (value) applyMonitors(value);
      })
      .catch((error) => {
        if (!disposed) console.error("Display measurement failed", error);
      });
    return () => {
      disposed = true;
      void geometrySubscription.then((unlisten) => unlisten());
      void monitorSubscription.then((unlisten) => unlisten());
    };
  }, [ownScreenId, enabled]);
  return { geometries, monitors };
}
