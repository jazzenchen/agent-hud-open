import { useEffect, type RefObject } from "react";
import { AnimatePresence } from "motion/react";
import { useFormat } from "@agent-hud/ui/format-context";
import type { HUDRect } from "../geometry";
import type { HUDConfig, HUDEdge } from "../model";
import type { HUDNativePointer } from "../scene/useHUDInteraction";
import { END_SIDES, type DockEndsGeometry, type EndSide } from "./end-geometry";
import { EndFlights } from "./EndFlights";
import { EndPill } from "./EndPill";
import type { DockEndsState } from "./useDockEnds";
import type { EndFade } from "./useParkedEnds";

const inside = (rect: HUDRect, point: { x: number; y: number }, pad = 0) =>
  point.x >= rect.x - pad &&
  point.x <= rect.x + rect.width + pad &&
  point.y >= rect.y - pad &&
  point.y <= rect.y + rect.height + pad;

/** The dock's two ends, the cards flying to them, and which end the pointer
 * is on: it stays the one whose panel shows until the pointer reaches the
 * dock or the HUD closes. */
export function DockEnds({
  ends,
  geometry,
  counts,
  fade,
  config,
  edge,
  strip,
  frame,
  cardVisible,
  showing,
  dragging,
  light,
  fill,
  outline,
  reduceMotion,
  nativePointer,
  scene,
  collapsed,
  pointerInside,
  entered,
  domExited,
  forceOpen,
}: {
  ends: DockEndsState;
  geometry: DockEndsGeometry;
  counts: Record<EndSide, number>;
  fade: EndFade;
  config: HUDConfig;
  edge: HUDEdge;
  strip: HUDRect;
  frame: HUDRect;
  cardVisible: boolean;
  /** An end's panel is on show. */
  showing: boolean;
  dragging: boolean;
  light: boolean;
  fill: string;
  outline: string;
  reduceMotion: boolean;
  nativePointer?: HUDNativePointer;
  scene: RefObject<HTMLElement | null>;
  collapsed: boolean;
  pointerInside: boolean;
  entered: (optionDown?: boolean) => void;
  domExited: (event?: { relatedTarget?: EventTarget | null }) => void;
  forceOpen: () => void;
}) {
  const zh = useFormat().language === "zh";
  const notch = config.mode === "notch";
  // The notch becomes the card: its ends wait in its row till it is back.
  const hidden = dragging || (notch && cardVisible);
  const { setHovered } = ends;
  useEffect(() => {
    ends.cardFrame.current = cardVisible && !showing ? frame : null;
  });
  // On the dock itself the dock's own panel takes over.
  const onDock = (point: { x: number; y: number }) =>
    inside(strip, point) && !(notch && cardVisible);
  useEffect(() => {
    const point = nativePointer?.point;
    if (!nativePointer?.inside || !point) return;
    const side = END_SIDES.find(
      (s) => !hidden && counts[s] > 0 && inside(geometry[s], point, 3),
    );
    if (side) setHovered(side);
    else if (onDock(point)) setHovered(null);
  }, [nativePointer]);
  useEffect(() => {
    const element = scene.current;
    if (nativePointer || !element) return;
    const over = (event: PointerEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      const side = target?.closest<HTMLElement>(".hud-dock-end")?.dataset.end;
      if (side === "head" || side === "tail") setHovered(side);
      else if (target?.closest(".hud-resting-strip") && !(notch && cardVisible))
        setHovered(null);
    };
    element.addEventListener("pointerover", over);
    return () => element.removeEventListener("pointerover", over);
  }, [Boolean(nativePointer), notch, cardVisible]);
  useEffect(() => {
    if (collapsed && !pointerInside) setHovered(null);
  }, [collapsed, pointerInside]);

  const rowDark = notch && config.hasNotch;
  const across = notch || edge === "top" || edge === "bottom";
  return (
    <>
      <AnimatePresence>
        {!hidden &&
          END_SIDES.filter((side) => counts[side] > 0).map((side) => (
            <EndPill
              key={side}
              side={side}
              rect={geometry[side]}
              count={counts[side]}
              mode={config.mode}
              across={across}
              light={rowDark ? false : light}
              fill={fill}
              outline={outline}
              fade={fade}
              reduceMotion={reduceMotion}
              onEnter={(altKey) => {
                if (nativePointer) return;
                setHovered(side);
                entered(altKey);
              }}
              onLeave={domExited}
              onPick={() => {
                setHovered(side);
                forceOpen();
              }}
              zh={zh}
            />
          ))}
      </AnimatePresence>
      <EndFlights
        flights={ends.flights}
        rects={geometry}
        edge={edge}
        thick={geometry.thick}
        ground={rowDark ? "#000" : fill}
        outline={outline}
        light={rowDark ? false : light}
        land={ends.land}
      />
    </>
  );
}
