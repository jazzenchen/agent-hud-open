import { useLayoutEffect, useRef, useState, type RefObject } from "react";
import type { HUDRect } from "../geometry";
import type { HUDNativePointer } from "./useHUDInteraction";

function rectContains(rect: HUDRect, point: { x: number; y: number }) {
  return (
    point.x >= rect.x &&
    point.x <= rect.x + rect.width &&
    point.y >= rect.y &&
    point.y <= rect.y + rect.height
  );
}

/** When the card shrinks away from under the pointer, the space it covered
 * stays the HUD's (`heldFrame`, the hover floor's box) until the pointer
 * reaches the card or the strip, so the panel does not close under it.
 * `trackPointer` follows the pointer across the scene outside a native host. */
export function useHeldFrame({
  cardVisible,
  pointerInside,
  dragging,
  frame,
  strip,
  nativePointer,
  scene,
}: {
  cardVisible: boolean;
  pointerInside: boolean;
  dragging: boolean;
  frame: HUDRect;
  strip: HUDRect;
  nativePointer?: HUDNativePointer;
  scene: RefObject<HTMLElement | null>;
}) {
  const [heldFrame, setHeldFrame] = useState<HUDRect | null>(null);
  const lastFrameRef = useRef<HUDRect | null>(null);
  const lastPointerPointRef = useRef<{ x: number; y: number } | null>(null);
  useLayoutEffect(() => {
    if (!cardVisible || !pointerInside || dragging) {
      lastFrameRef.current = cardVisible ? frame : null;
      setHeldFrame((current) => (current ? null : current));
      return;
    }
    const prev = heldFrame ?? lastFrameRef.current;
    lastFrameRef.current = frame;
    if (!prev) return;
    const minX = Math.min(prev.x, frame.x);
    const minY = Math.min(prev.y, frame.y);
    const maxX = Math.max(prev.x + prev.width, frame.x + frame.width);
    const maxY = Math.max(prev.y + prev.height, frame.y + frame.height);
    const shrunk =
      maxX - minX > frame.width + 1 || maxY - minY > frame.height + 1;
    if (!shrunk) {
      if (heldFrame) setHeldFrame(null);
      return;
    }
    const pt = nativePointer?.point ?? lastPointerPointRef.current;
    if (pt && (rectContains(frame, pt) || rectContains(strip, pt))) {
      if (heldFrame) setHeldFrame(null);
      return;
    }
    if (
      !heldFrame ||
      Math.abs(heldFrame.x - minX) > 0.5 ||
      Math.abs(heldFrame.y - minY) > 0.5 ||
      Math.abs(heldFrame.width - (maxX - minX)) > 0.5 ||
      Math.abs(heldFrame.height - (maxY - minY)) > 0.5
    ) {
      setHeldFrame({
        x: minX,
        y: minY,
        width: maxX - minX,
        height: maxY - minY,
      });
    }
  }, [
    cardVisible,
    pointerInside,
    dragging,
    frame.x,
    frame.y,
    frame.width,
    frame.height,
    strip.x,
    strip.y,
    strip.width,
    strip.height,
    nativePointer?.point?.x,
    nativePointer?.point?.y,
    heldFrame,
  ]);
  const trackPointer = (event: { clientX: number; clientY: number }) => {
    if (nativePointer) return;
    const bounds = scene.current?.getBoundingClientRect();
    if (!bounds) return;
    const pt = {
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top,
    };
    lastPointerPointRef.current = pt;
    if (heldFrame && (rectContains(frame, pt) || rectContains(strip, pt))) {
      setHeldFrame(null);
    }
  };
  return { heldFrame, trackPointer };
}
