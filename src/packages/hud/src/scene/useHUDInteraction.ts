import {
  useCallback,
  useEffect,
  useMemo,
  type MutableRefObject,
  type RefObject,
} from "react";
import type { Approval, IslandEvent } from "@agent-hud/protocol";
import { horizontal, type HUDRect, type StripMetrics } from "../geometry";
import type { HUDConfig, HUDEdge, HUDVendor } from "../model";
import { useAlertPresentation } from "./useAlertPresentation";
import type { HUDDragPreview } from "./useHUDDrag";

/** Native input is authoritative while a transparent host toggles mouse passthrough. */
export interface HUDNativePointer {
  inside: boolean;
  point: { x: number; y: number } | null;
  buttons: number | null;
  altKey: boolean | null;
  ctrlKey: boolean | null;
  metaKey: boolean | null;
  shiftKey: boolean | null;
}

export interface UseHUDInteractionArgs {
  config: HUDConfig;
  edge: HUDEdge;
  strip: HUDRect;
  metrics: StripMetrics;
  vendors: HUDVendor[];
  width: number;
  height: number;
  scene: RefObject<HTMLElement | null>;
  open: boolean;
  hoverPhase?: string;
  hovered: string | null;
  pointerInside: boolean;
  dragVisual: boolean;
  dragPreview?: HUDDragPreview | null;
  dragCandidate: MutableRefObject<{ armed: boolean; started: boolean } | null>;
  suppressDragClick: MutableRefObject<boolean>;
  suppressHover: MutableRefObject<boolean>;
  hoverInside: MutableRefObject<boolean>;
  enter: () => void;
  leave: () => void;
  forceOpen: () => void;
  collapse: () => void;
  setHovered: (value: string | null) => void;
  setPinned: (value: boolean) => void;
  setPointerInside: (value: boolean) => void;
  request?: Approval;
  event?: IslandEvent;
  inUsagePanel?: boolean;
  typing?: boolean;
  nativePointer?: HUDNativePointer;
  onConfigure?: () => void;
  onStats?: () => void;
  /** Escape only ends an agent's edit hold; the panel is left to the hover. */
  onEscape?: () => void;
}

export function useHUDInteraction({
  config,
  edge,
  strip,
  metrics,
  vendors,
  width,
  height,
  scene,
  open,
  hoverPhase,
  hovered,
  pointerInside,
  dragVisual,
  dragPreview,
  dragCandidate,
  suppressDragClick,
  suppressHover,
  hoverInside,
  enter,
  leave,
  forceOpen,
  collapse,
  setHovered,
  setPinned,
  setPointerInside,
  request,
  event,
  inUsagePanel = false,
  typing = false,
  nativePointer,
  onConfigure,
  onStats,
  onEscape,
}: UseHUDInteractionArgs) {
  useEffect(() => {
    if (typing) {
      forceOpen();
    } else if (!pointerInside) {
      leave();
    }
  }, [typing]);

  useEffect(() => {
    if (!open) setPinned(false);
  }, [open, setPinned]);

  useEffect(() => {
    collapse();
    setPinned(false);
    setHovered(null);
  }, [config.mode, config.edge, config.separatesAgents]);

  useAlertPresentation({
    config,
    open,
    pointerInside,
    typing,
    dragVisual,
    dragCandidate,
    suppressHover,
    enter,
    leave,
    forceOpen,
    collapse,
    setPinned,
    request,
    event,
    inUsagePanel,
  });

  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") onEscape?.();
      if (event.key === "Alt" && pointerInside && !suppressHover.current)
        enter();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [enter, pointerInside, suppressHover, onEscape]);

  function vendorUnderPointer(event: { clientX: number; clientY: number }) {
    const bounds = scene.current?.getBoundingClientRect();
    if (!bounds || config.mode === "notch") return null;
    const x = event.clientX - bounds.left - strip.x,
      y = event.clientY - bounds.top - strip.y;
    if (x < 0 || y < 0 || x > strip.width || y > strip.height) return null;
    const along = horizontal(edge) ? x : y;
    let index = -1,
      distance = Infinity;
    metrics.centers.forEach((center, candidate) => {
      if (Math.abs(center - along) < distance) {
        index = candidate;
        distance = Math.abs(center - along);
      }
    });
    return vendors[index]?.key ?? null;
  }

  function followMark(event: { clientX: number; clientY: number }) {
    if (dragCandidate.current || dragPreview) return;
    const key = vendorUnderPointer(event);
    if (key && key !== hovered) {
      setHovered(key);
    }
  }

  /** At the very top of the screen, the pointer opens the panel at once. */
  function openAtTop(event: {
    clientX: number;
    clientY: number;
    altKey: boolean | null;
  }) {
    if (
      !config.openImmediatelyAtTop ||
      edge !== "top" ||
      (hoverPhase !== "opening" && hoverPhase !== "collapsed") ||
      suppressHover.current ||
      (config.requiresOptionToOpen && !event.altKey && !hoverInside.current)
    )
      return;
    const bounds = scene.current?.getBoundingClientRect();
    const x = event.clientX - (bounds?.left ?? 0);
    const y = event.clientY - (bounds?.top ?? 0);
    if (x >= strip.x && x <= strip.x + strip.width && y <= 1) forceOpen();
  }

  function entered(optionDown = false) {
    setPointerInside(true);
    if (
      !suppressHover.current &&
      !dragVisual &&
      (open ||
        hoverInside.current ||
        !config.requiresOptionToOpen ||
        optionDown)
    )
      enter();
  }

  function exited() {
    setPointerInside(false);
    if (!dragCandidate.current && !dragPreview) suppressHover.current = false;
    if (!typing) leave();
  }

  useEffect(() => {
    if (!nativePointer) return;
    if (!nativePointer.inside || !nativePointer.point) {
      exited();
      return;
    }
    if (dragVisual || dragCandidate.current) return;
    const bounds = scene.current?.getBoundingClientRect();
    const client = {
      clientX: nativePointer.point.x + (bounds?.left ?? 0),
      clientY: nativePointer.point.y + (bounds?.top ?? 0),
    };
    followMark(client);
    entered(nativePointer.altKey === true);
    openAtTop({ ...client, altKey: nativePointer.altKey });
  }, [
    nativePointer,
    dragVisual,
    config.mode,
    edge,
    hoverPhase,
    strip.x,
    strip.width,
    config.requiresOptionToOpen,
    config.openImmediatelyAtTop,
    width,
    height,
  ]);

  const domExited = (event?: { relatedTarget?: EventTarget | null }) => {
    if (nativePointer) return;
    if (
      event?.relatedTarget instanceof Element &&
      event.relatedTarget.closest(
        ".hud-resting-strip, .hud-expanded-surface, .hud-grab-surface, .hud-hover-floor",
      )
    ) {
      return;
    }
    exited();
  };

  function pin(key?: string) {
    if (
      dragCandidate.current?.armed ||
      dragPreview ||
      suppressDragClick.current
    )
      return;
    if (!config.separatesAgents || config.mode === "notch" || request || event)
      return;
    if (key) setHovered(key);
    setPinned(true);
    forceOpen();
  }

  const handoff = useCallback(
    (callback?: () => void) => {
      suppressHover.current = true;
      collapse();
      callback?.();
    },
    [collapse, suppressHover],
  );
  const configurePanel = useMemo(
    () => (onConfigure ? () => handoff(onConfigure) : undefined),
    [onConfigure, handoff],
  );
  const showPanelStats = useMemo(
    () => (onStats ? () => handoff(onStats) : undefined),
    [onStats, handoff],
  );

  return {
    vendorUnderPointer,
    followMark,
    entered,
    domExited,
    pin,
    openAtTop,
    handoff,
    configurePanel,
    showPanelStats,
  };
}
