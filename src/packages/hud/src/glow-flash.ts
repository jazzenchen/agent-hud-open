import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type AnimationEvent,
} from "react";
import {
  afterFlash,
  flashAnimation,
  flashKeyframes,
  flashMask,
  type GlowAlert,
} from "./glow-alert";
import type { RimShape, RimTrack } from "./rim-track";

/** The flash's @keyframes, added to the page once. A CSS animation, not
 * element.animate(): WebKit never frees an element that has run a script-made
 * animation, so every alert's flash layer, its canvas included, stayed in the
 * HUD's memory (issue 47). */
let keyframesAdded = false;
function addKeyframes() {
  if (keyframesAdded) return;
  keyframesAdded = true;
  const style = document.createElement("style");
  style.dataset.glowFlash = "";
  style.textContent = flashKeyframes();
  document.head.append(style);
}

/** Each alert flashes once, when it arrives, in its colour over its sender's
 * part of the moving silhouette: the element to flash, the alert flashing,
 * and the flash's mask for the silhouette where it is. */
export function useGlowFlash({
  alert,
  reduced,
  shape,
  padding,
  width,
}: {
  alert: GlowAlert | null;
  reduced: boolean;
  shape: RimTrack;
  padding: number;
  width: number;
}) {
  const flash = useRef<HTMLDivElement | null>(null);
  const [flashing, setFlashing] = useState<GlowAlert | null>(null);
  useEffect(addKeyframes, []);
  useEffect(() => {
    // A new alert flashes; one that is gone stops its flash at once.
    setFlashing(alert);
  }, [alert?.id]);
  // Chosen when the flash starts. The layer is keyed by the alert, so a new
  // alert starts its animation on a layer of its own.
  const animation = useMemo(
    () => (flashing ? flashAnimation(flashing.warning, reduced) : undefined),
    [flashing?.id],
  );
  const ended = (event: AnimationEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget || !flashing) return;
    const id = flashing.id;
    setFlashing((current) => afterFlash(current, id));
  };
  const maskAt = (at: RimShape) =>
    flashing
      ? flashMask(flashing.span, at.width, padding, width, at.x)
      : undefined;
  const latestMask = useRef(maskAt);
  latestMask.current = maskAt;
  // The flash covers its sender's part of the moving silhouette.
  useLayoutEffect(() => {
    if (!flashing) return;
    return shape.subscribe(() => {
      const element = flash.current;
      if (!element) return;
      const mask = latestMask.current(shape.get()) ?? "";
      element.style.maskImage = mask;
      element.style.setProperty("-webkit-mask-image", mask);
    });
  }, [flashing?.id, shape]);
  return {
    flash,
    flashing,
    maskAt,
    flashAnimation: animation,
    flashEnded: ended,
  };
}
