import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { flashMask, pulseFrames, type GlowAlert } from "./glow-alert";
import type { RimShape, RimTrack } from "./rim-track";

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
  useEffect(() => {
    // A new alert flashes; one that is gone stops its flash at once.
    setFlashing(alert);
  }, [alert?.id]);
  useEffect(() => {
    const element = flash.current;
    if (!flashing || !element) return;
    const animation = element.animate(pulseFrames(flashing.warning, reduced), {
      duration: 1800,
      easing: "linear",
      fill: "forwards",
    });
    animation.onfinish = () =>
      setFlashing((current) => (current?.id === flashing.id ? null : current));
    return () => animation.cancel();
  }, [flashing?.id]);
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
  return { flash, flashing, maskAt };
}
