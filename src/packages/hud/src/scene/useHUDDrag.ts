import {
  useEffect,
  useRef,
  useState,
  type MutableRefObject,
  type PointerEvent as ReactPointerEvent,
  type RefObject,
} from "react";
import type { Snapshot } from "@agent-hud/protocol";
import { stripMetrics, type HUDRect } from "../geometry";
import { parkedRect } from "../placement";
import { configuredVendors, type HUDConfig, type HUDEdge } from "../model";

export interface HUDDragStartArgs {
  mode: HUDConfig["mode"];
  edge: HUDEdge;
  /** Scene-local CSS pixels, captured when the primary pointer goes down. */
  strip: HUDRect;
  pointer: { x: number; y: number };
  /** Target-mode runs for native snapping on horizontal and vertical edges. */
  runs: { horizontal: number; vertical: number };
}

export type HUDDragPreview = Pick<
  HUDConfig,
  "mode" | "edge" | "offset" | "monitorIndex"
>;

interface DragCandidate {
  pointerId: number;
  x: number;
  y: number;
  armed: boolean;
  started: boolean;
  timer: ReturnType<typeof setTimeout>;
  detach: () => void;
}

export function useHUDDrag({
  snapshot,
  sourceConfig,
  notchFrame,
  screen,
  scene,
  onDragStart,
  dragPreview,
  dragEndSequence,
  open,
  forceOpen,
  leave,
  collapse,
  setPointerInside,
  setPinned,
  setHovered,
  suppressHover,
}: {
  snapshot: Snapshot;
  sourceConfig: HUDConfig;
  notchFrame?: HUDRect;
  screen: { width: number; height: number };
  scene: RefObject<HTMLElement | null>;
  onDragStart?: (args: HUDDragStartArgs) => void | Promise<void>;
  dragPreview?: HUDDragPreview | null;
  dragEndSequence: number;
  open: boolean;
  forceOpen: () => void;
  leave: () => void;
  collapse: () => void;
  setPointerInside: (inside: boolean) => void;
  setPinned: (pinned: boolean) => void;
  setHovered: (key: string | null) => void;
  suppressHover: MutableRefObject<boolean>;
}) {
  const [dragOutlined, setDragOutlined] = useState(false);
  const dragCandidate = useRef<DragCandidate | null>(null);
  const suppressDragClick = useRef(false);

  const dragVisual = dragOutlined || Boolean(dragPreview);
  const dragging =
    Boolean(dragPreview) || Boolean(dragCandidate.current?.started);

  function cancelDragCandidate() {
    suppressHover.current = false;
    setDragOutlined(false);
    const candidate = dragCandidate.current;
    if (!candidate) return;
    clearTimeout(candidate.timer);
    candidate.detach();
    dragCandidate.current = null;
  }

  useEffect(
    () => () => cancelDragCandidate(),
    [sourceConfig.mode, sourceConfig.edge, Boolean(onDragStart)],
  );

  useEffect(() => {
    const pickedUp = dragCandidate.current?.started;
    cancelDragCandidate();
    if (pickedUp) suppressHover.current = true;
  }, [dragEndSequence]);

  function considerDrag(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.button !== 0 || !event.isPrimary) return;
    if (dragPreview || dragCandidate.current?.started) return;
    cancelDragCandidate();
    suppressDragClick.current = false;
    if (!onDragStart) return;
    const bounds = scene.current?.getBoundingClientRect();
    if (!bounds) return;
    const start = onDragStart;
    suppressHover.current = true;
    if (open) forceOpen();
    else {
      leave();
      collapse();
    }
    const sourceVendors = configuredVendors(snapshot, sourceConfig);
    const sourceMetrics = stripMetrics(
      sourceConfig,
      sourceVendors,
      snapshot.settings.quotaDisplay,
    );
    const sourceStrip =
      sourceConfig.mode === "notch" && notchFrame
        ? notchFrame
        : parkedRect(screen, sourceMetrics, sourceConfig);
    // A notch dragged away from the notch becomes a Simple dock.
    const targetMode = "simple";
    const targetVendors = configuredVendors(snapshot, {
      ...sourceConfig,
      mode: targetMode,
    });
    const args: HUDDragStartArgs = {
      mode: sourceConfig.mode,
      edge: sourceConfig.mode === "notch" ? "top" : sourceConfig.edge,
      strip: { ...sourceStrip },
      pointer: {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      },
      runs: {
        horizontal: stripMetrics(
          { ...sourceConfig, mode: targetMode, edge: "top" },
          targetVendors,
          snapshot.settings.quotaDisplay,
        ).width,
        vertical: stripMetrics(
          { ...sourceConfig, mode: targetMode, edge: "right" },
          targetVendors,
          snapshot.settings.quotaDisplay,
        ).height,
      },
    };
    const candidate: DragCandidate = {
      pointerId: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      armed: false,
      started: false,
      timer: setTimeout(arm, 300),
      detach: () => {
        window.removeEventListener("pointermove", moved, true);
        window.removeEventListener("pointerup", released, true);
        window.removeEventListener("pointercancel", released, true);
        window.removeEventListener("blur", blurred);
      },
    };
    dragCandidate.current = candidate;
    window.addEventListener("pointermove", moved, true);
    window.addEventListener("pointerup", released, true);
    window.addEventListener("pointercancel", released, true);
    window.addEventListener("blur", blurred);

    function arm() {
      if (dragCandidate.current !== candidate || candidate.armed) return;
      clearTimeout(candidate.timer);
      candidate.armed = true;
      suppressDragClick.current = true;
      setDragOutlined(true);
    }
    function begin() {
      if (dragCandidate.current !== candidate || candidate.started) return;
      arm();
      candidate.started = true;
      clearTimeout(candidate.timer);
      suppressDragClick.current = true;
      suppressHover.current = true;
      setPointerInside(false);
      leave();
      collapse();
      setPinned(false);
      setHovered(null);
      // Only the host moves the native window or commits a snapped placement.
      void Promise.resolve()
        .then(() => start(args))
        .catch((error) => {
          if (dragCandidate.current === candidate) cancelDragCandidate();
          console.error("Native HUD drag could not start", error);
        });
    }
    function moved(move: PointerEvent) {
      if (move.pointerId !== candidate.pointerId || candidate.started) return;
      if (
        candidate.armed ||
        Math.hypot(move.clientX - candidate.x, move.clientY - candidate.y) >= 6
      )
        begin();
    }
    function released(release: PointerEvent) {
      if (release.pointerId === candidate.pointerId && !candidate.started)
        cancelDragCandidate();
    }
    function blurred() {
      // Once picked up, only a real release or the native end sequence ends it.
      if (!candidate.started) cancelDragCandidate();
    }
  }

  return {
    dragVisual,
    dragging,
    dragCandidate,
    suppressDragClick,
    cancelDragCandidate,
    considerDrag,
  };
}
