import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { horizontal, type HUDRect } from "../geometry";
import { HUD_METRICS } from "../model";
import { expandedOrigin } from "./hover-hit";
import type { useNotchGeometry } from "./NotchSurface";
import { outsideSimplePanel } from "./simple-panel";
import { surfaceBorder } from "./simple-panel";
import { SurfaceContent, type SurfaceContentProps } from "./SurfaceContent";

export function HUDExpandedSurface({
  config,
  edge,
  cardVisible,
  hoverPhase,
  detached,
  peek,
  simplePeek,
  open,
  reduceMotion,
  dragging,
  frame,
  strip,
  flared,
  surfaceFlare,
  metricsRadius,
  targetSize,
  notchGeometry,
  light,
  fill,
  outline,
  shadow,
  nativePointer,
  entered,
  domExited,
  ...content
}: Omit<SurfaceContentProps, "rowLight" | "active"> & {
  cardVisible: boolean;
  hoverPhase?: string;
  detached: boolean;
  simplePeek: boolean;
  dragging: boolean;
  strip: HUDRect;
  flared: boolean;
  metricsRadius: number;
  targetSize: { width: number; height: number };
  notchGeometry: ReturnType<typeof useNotchGeometry>;
  fill: string;
  outline: string;
  /** The ground's shadow, fading with its transparency. */
  shadow: string;
  nativePointer?: unknown;
  entered: (optionDown?: boolean) => void;
  domExited: (event?: { relatedTarget?: EventTarget | null }) => void;
}) {
  // A hardware notch's row stays dark whatever the appearance, at rest and
  // while a reminder shows its wings, so it stays one with the camera
  // housing: black. What opens out of the row takes the look itself (Open
  // HUDLook.inNotchRow). A closing panel keeps its own look while it shrinks and the row's takes over once it is
  // back in the row; at once, a light panel would turn into a black block.
  const atNotchRow =
    config.mode === "notch" && config.hasNotch && !open && !peek;
  const settle = reduceMotion ? 0 : HUD_METRICS.duration;
  const [rowSettled, setRowSettled] = useState(atNotchRow);
  useEffect(() => {
    if (!atNotchRow) {
      setRowSettled(false);
      return;
    }
    if (settle === 0) {
      setRowSettled(true);
      return;
    }
    const timer = setTimeout(() => setRowSettled(true), settle * 1000);
    return () => clearTimeout(timer);
  }, [atNotchRow, settle]);
  const inNotchRow = atNotchRow && rowSettled;
  const rowLight = inNotchRow ? false : light;
  const transition = {
    duration: reduceMotion || dragging ? 0 : 0.4,
    ease: HUD_METRICS.ease,
  };
  // Pre-warm the React tree during the hover delay (`opening`) so Frame 0 of
  // expansion only flips composited opacity/scale instead of cold-mounting.
  const prewarming = !dragging && !cardVisible && hoverPhase === "opening";
  const renderContent = cardVisible || prewarming;
  const closedPose = {
    scaleX:
      detached && simplePeek && !horizontal(edge)
        ? 0.2
        : detached
          ? 0.86
          : horizontal(edge)
            ? 0.92
            : 0.88,
    scaleY:
      detached && simplePeek && !horizontal(edge)
        ? 1
        : detached
          ? 0.86
          : horizontal(edge)
            ? 0.88
            : 0.92,
    opacity: 0,
  };
  const targetHit = cardVisible
    ? frame
    : config.mode === "notch"
      ? strip
      : { x: frame.x, y: frame.y, width: 0, height: 0 };

  // What opens beside a Simple dock is cut where the dock is: it grows from behind the dock, the dock keeps its own
  // ground whatever is open, and over a see-through look the two would darken each other (Open
  // IslandRootView.outsidePanel). The usage panel too: it can be shorter than the dock, which then reaches past it.
  const cut =
    config.mode === "simple"
      ? outsideSimplePanel(strip, edge, metricsRadius)
      : undefined;

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        zIndex: 2,
        pointerEvents: "none",
        clipPath: cut,
      }}
    >
      <AnimatePresence initial={false} custom={dragging}>
        {(config.mode === "notch" || renderContent) && (
          <motion.div
            key={config.mode === "notch" ? "notch-shell" : "hud-card"}
            className={`hud-expanded-surface ${detached ? "hud-detached-card" : ""} ${peek ? "hud-peek" : ""}`}
            data-target-x={targetHit.x}
            data-target-y={targetHit.y}
            data-target-width={targetHit.width}
            data-target-height={targetHit.height}
            initial={
              config.mode === "notch" || reduceMotion ? false : closedPose
            }
            animate={
              config.mode === "notch"
                ? { opacity: 1 }
                : cardVisible
                  ? { scaleX: 1, scaleY: 1, opacity: 1 }
                  : { ...closedPose, transition: { duration: 0 } }
            }
            variants={{
              gone: (moving: boolean) =>
                moving
                  ? { opacity: 0, transition: { duration: 0 } }
                  : reduceMotion
                    ? { opacity: 0 }
                    : closedPose,
            }}
            exit="gone"
            transition={transition}
            onPointerEnter={(e) => {
              if (!nativePointer && cardVisible) entered(e.altKey);
            }}
            onPointerLeave={domExited}
            style={{
              position: "absolute",
              ...(config.mode === "notch"
                ? {
                    left: notchGeometry.x,
                    top: notchGeometry.y,
                    width: notchGeometry.width,
                    height: notchGeometry.height,
                  }
                : {
                    left: frame.x,
                    top: frame.y,
                    width: frame.width,
                    height: frame.height,
                    willChange: "transform, opacity",
                  }),
              zIndex: 2,
              pointerEvents:
                config.mode === "notch" || cardVisible ? "auto" : "none",
              transformOrigin: expandedOrigin(edge, frame, strip),
              overflow: config.mode === "notch" ? "visible" : "hidden",
              borderRadius: detached
                ? Math.min(
                    peek ? (simplePeek ? targetSize.height / 2 : 14) : 22,
                    targetSize.height / 2,
                  )
                : config.mode === "simple"
                  ? 26
                  : 0,
              background:
                detached || config.mode === "simple" ? fill : "transparent",
              // The Simple line is the panel growing: no outline of its own.
              border: surfaceBorder(config.mode, detached, simplePeek)
                ? `${surfaceBorder(config.mode, detached, simplePeek)}px solid ${outline}`
                : "none",
              boxShadow: config.mode === "notch" ? undefined : shadow,
            }}
          >
            {config.mode === "notch" && (
              <svg
                aria-hidden="true"
                style={{
                  position: "absolute",
                  inset: 0,
                  width: "100%",
                  height: "100%",
                  overflow: "visible",
                  pointerEvents: "none",
                  // A layer of its own. Painted into the window's layer, WebKit does not clear the shadow below the
                  // shape when it shrinks, and grey bands stay on screen after the panel closes.
                  willChange: "transform",
                  filter:
                    cardVisible || !config.hasNotch
                      ? "drop-shadow(0 9px 15px #00000022)"
                      : undefined,
                }}
              >
                <motion.path
                  d={notchGeometry.path}
                  fill={inNotchRow ? "#000" : fill}
                  stroke={outline}
                  strokeWidth={0.7}
                />
              </svg>
            )}
            <AnimatePresence initial={false}>
              {renderContent && (
                <motion.div
                  key="surface-content"
                  initial={false}
                  animate={{
                    opacity: cardVisible ? 1 : 0,
                    transition: { duration: 0 },
                  }}
                  exit={{
                    opacity: 0,
                    transition: { duration: reduceMotion ? 0 : 0.1 },
                  }}
                  style={{
                    position: "relative",
                    width: "100%",
                    height: "100%",
                    pointerEvents: cardVisible ? "auto" : "none",
                    visibility: cardVisible ? "visible" : "hidden",
                    clipPath:
                      config.mode === "notch" ? notchGeometry.clip : undefined,
                  }}
                >
                  <SurfaceContent
                    config={config}
                    edge={edge}
                    frame={frame}
                    peek={peek}
                    open={open}
                    active={cardVisible}
                    reduceMotion={reduceMotion}
                    surfaceFlare={surfaceFlare}
                    light={light}
                    rowLight={rowLight}
                    {...content}
                  />
                </motion.div>
              )}
            </AnimatePresence>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
