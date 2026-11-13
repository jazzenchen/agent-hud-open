import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { HUDScene } from "@agent-hud/hud/HUDScene";
import type {
  HUDDragStartArgs,
  HUDFlight,
  HUDFlightCommand,
} from "@agent-hud/hud/HUDScene";
import type {
  HUDConfig,
  HUDDecision,
  HUDUpdateNotice,
} from "@agent-hud/hud/model";
import type { Direction } from "@agent-hud/hud/bots/mark-director";
import type { Snapshot, NativeMethod } from "@agent-hud/protocol";
import type {
  NativeHUDGeometry,
  NativeHUDMonitor,
} from "./lib/use-hud-geometry";
import { useHUDPointer } from "./lib/use-hud-pointer";
import { useHUDTextFocus } from "./lib/use-hud-text-focus";
import type { ProductGuide } from "./guide/plan";
import { openSettingsSteps } from "./guide/steps";
import { openNews } from "./guide/news";
import { useHUDGuide } from "./guide/use-hud-guide";

export default function NativeHUDSurface({
  screenId,
  snapshot,
  config,
  geometry,
  monitors,
  botDirections,
  glowHidden,
  action,
  updateConfig,
  demo = false,
  guide: product,
}: {
  screenId: string;
  snapshot: Snapshot;
  config: HUDConfig;
  geometry: NativeHUDGeometry | null;
  monitors: NativeHUDMonitor[];
  botDirections?: Record<string, Direction>;
  glowHidden?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  /** Changes this screen's HUD configuration, as Settings' Display page does. */
  updateConfig: (patch: Partial<HUDConfig>) => unknown;
  demo?: boolean;
  guide?: ProductGuide;
}) {
  const nativePointer = useHUDPointer(screenId);
  const textFocused = useHUDTextFocus(
    screenId,
    snapshot.approvals,
    nativePointer?.inside,
  );
  const [dragEndSequence, setDragEndSequence] = useState(0);
  const lastPreview = useRef<Pick<
    HUDConfig,
    "mode" | "edge" | "offset"
  > | null>(null);
  // After a drop the saved placement reaches this window through Settings a moment later.
  // Keep showing the dropped placement until then, instead of jumping back to the old one.
  const [held, setHeld] = useState<Pick<
    HUDConfig,
    "mode" | "edge" | "offset"
  > | null>(null);
  const [dragPreview, setDragPreview] = useState<Pick<
    HUDConfig,
    "mode" | "edge" | "offset" | "monitorIndex"
  > | null>(null);
  useEffect(() => {
    if (!held) return;
    if (
      config.mode === held.mode &&
      config.edge === held.edge &&
      Math.abs(config.offset - held.offset) < 1e-6
    ) {
      setHeld(null);
      return;
    }
    const timer = setTimeout(() => setHeld(null), 800);
    return () => clearTimeout(timer);
  }, [held, config.mode, config.edge, config.offset]);
  const shownConfig = held ? { ...config, ...held } : config;
  const configure = useCallback(() => {
    void invoke("hud_show_main");
  }, []);
  // Statistics opens on Tokens (Open's footer button), or where a session row points.
  const stats = useCallback((target?: { tab?: string; sessionId?: string }) => {
    void invoke("open_statistics", {
      tab: target?.tab ?? "tokens",
      sessionId: target?.sessionId,
    });
  }, []);
  const looked = useCallback((keys: string[] | null) => {
    void invoke("hud_bot_looked", { keys });
  }, []);
  const beginDrag = useCallback(async (args: HUDDragStartArgs) => {
    await invoke("hud_drag_start", { ...args });
  }, []);
  const decide = useCallback(
    async (id: string, decision: HUDDecision) => {
      await action("answer_request", { id, decision });
    },
    [action],
  );
  // `--demo-script` panel steps; only the primary screen's HUD follows them.
  const [panelCommand, setPanelCommand] = useState<{
    open: boolean;
    sequence: number;
  }>();
  useEffect(() => {
    if (!isTauri()) return;
    const subscription = listen<boolean>("demo:panel", ({ payload }) =>
      setPanelCommand((previous) => ({
        open: payload,
        sequence: (previous?.sequence ?? 0) + 1,
      })),
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
  // The product's newer version (the panel footer's arrow): read when the
  // page starts, as a hidden page misses the event, then kept up to date.
  const [update, setUpdate] = useState<HUDUpdateNotice | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let heard = false;
    const subscription = listen<HUDUpdateNotice | null>(
      "hud:update",
      ({ payload }) => {
        heard = true;
        setUpdate(payload);
      },
    );
    void invoke<HUDUpdateNotice | null>("hud_update_notice").then((notice) => {
      if (!heard) setUpdate(notice);
    });
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
  const openUpdate = useCallback(() => {
    void invoke("open_update");
  }, []);
  // The guide's window: only beside the main screen's HUD.
  const primary =
    monitors.find((m) => m.screenId === screenId)?.primary ??
    monitors[0]?.screenId === screenId;
  const guide = useHUDGuide({
    active: primary,
    snapshot,
    config,
    update: updateConfig,
    action,
    demo,
    screens: monitors.length,
    news: [...openNews, ...(product?.news ?? [])],
    steps: [...openSettingsSteps, ...(product?.steps ?? [])],
  });
  // A product's flights between two agents' marks; every screen's HUD plays them.
  const [flights, setFlights] = useState<readonly HUDFlightCommand[]>([]);
  useEffect(() => {
    if (!isTauri()) return;
    const subscription = listen<HUDFlight>("hud:flight", ({ payload }) =>
      setFlights((previous) => [
        ...previous.slice(-15),
        {
          flight: payload,
          sequence: (previous.at(-1)?.sequence ?? 0) + 1,
          at: performance.now(),
        },
      ]),
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    const previewSubscription = listen<
      { screenId: string } & Pick<
        HUDConfig,
        "mode" | "edge" | "offset" | "monitorIndex"
      >
    >("hud:drag-preview", (event) => {
      if (disposed || event.payload.screenId !== screenId) return;
      const { mode, edge, offset, monitorIndex } = event.payload;
      lastPreview.current = { mode, edge, offset };
      setDragPreview({ mode, edge, offset, monitorIndex });
    });
    const subscription = listen<{ screenId: string; cancelled: boolean }>(
      "hud:drag-end",
      (event) => {
        if (!disposed && event.payload.screenId === screenId) {
          if (!event.payload.cancelled && lastPreview.current)
            setHeld(lastPreview.current);
          lastPreview.current = null;
          setDragPreview(null);
          setDragEndSequence((sequence) => sequence + 1);
        }
      },
    );
    return () => {
      disposed = true;
      void previewSubscription.then((unlisten) => unlisten());
      void subscription.then((unlisten) => unlisten());
    };
  }, [screenId]);
  if (isTauri() && (!geometry || geometry.screenId !== screenId)) return null;
  const underPointer = (() => {
    if (!isTauri() || !geometry || monitors.length <= 1) return true;
    if (!nativePointer?.point) return primary;
    const globalX = nativePointer.point.x + geometry.canvas.x;
    const globalY = nativePointer.point.y + geometry.canvas.y;
    const screen = geometry.screen;
    if (
      globalX >= screen.x &&
      globalX <= screen.x + screen.width &&
      globalY >= screen.y &&
      globalY <= screen.y + screen.height
    ) {
      return true;
    }
    for (const monitor of monitors) {
      if (monitor.screenId === screenId) continue;
      const scale =
        geometry.coordinateSpace === "global-logical-top-left"
          ? 1
          : monitor.scaleFactor || 1;
      const mx = monitor.bounds.x / scale;
      const my = monitor.bounds.y / scale;
      const mw = monitor.bounds.width / scale;
      const mh = monitor.bounds.height / scale;
      if (
        globalX >= mx &&
        globalX <= mx + mw &&
        globalY >= my &&
        globalY <= my + mh
      ) {
        return false;
      }
    }
    return primary;
  })();
  const notchFrame = geometry
    ? (geometry.notchInViewport ?? {
        x:
          geometry.screen.x +
          (geometry.screen.width - 200) / 2 -
          geometry.canvas.x,
        y: geometry.screen.y - geometry.canvas.y,
        width: 200,
        height: geometry.menuBarHeight,
      })
    : undefined;
  return (
    <HUDScene
      snapshot={snapshot}
      config={shownConfig}
      dragPreview={dragPreview}
      dragEndSequence={dragEndSequence}
      nativePointer={nativePointer}
      underPointer={underPointer}
      textFocused={textFocused}
      botDirections={botDirections}
      glowHidden={glowHidden}
      onBotLooked={looked}
      notchFrame={notchFrame}
      height={geometry?.canvas.height ?? window.innerHeight}
      onConfigure={configure}
      onStats={stats}
      update={update}
      onUpdate={openUpdate}
      onDragStart={beginDrag}
      onRequestDecision={decide}
      panelCommand={primary ? panelCommand : undefined}
      flights={flights}
      guide={guide}
    />
  );
}
