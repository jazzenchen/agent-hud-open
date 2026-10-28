import { useEffect, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import type {
  Approval,
  IslandEvent,
  RequestDelivery,
  Snapshot,
} from "@agent-hud/protocol";
import { HUDArtwork as HUDLogo } from "../HUDArtwork";
import { CompactAlertView } from "../IslandNews";
import { RequestDeliveryNotice } from "../permissions/ApprovalContent";
import { SourceHoverPanel } from "../HoverPanel";
import type { HUDRect } from "../geometry";
import {
  HUD_METRICS,
  type HUDConfig,
  type HUDEdge,
  type HUDVendor,
} from "../model";
import { AgentPeek } from "./AgentPeek";
import { Silhouette, type useNotchGeometry } from "./NotchSurface";
import { outsideSimplePanel } from "./simple-panel";
import { snapshotLanguage } from "@agent-hud/ui/format";

const horizontal = (edge: HUDEdge) => edge === "top" || edge === "bottom";

export function HUDExpandedSurface({
  config,
  edge,
  cardVisible,
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
  focusedVendor,
  scale,
  peekTuck,
  pin,
  request,
  event,
  requests,
  snapshot,
  forceOpen,
  dismissNews,
  showPanelStats,
  configurePanel,
  surfacePadding,
  eventContent,
  inUsagePanel,
  delivery,
}: {
  config: HUDConfig;
  edge: HUDEdge;
  cardVisible: boolean;
  detached: boolean;
  peek: boolean;
  simplePeek: boolean;
  open: boolean;
  reduceMotion: boolean;
  dragging: boolean;
  frame: HUDRect;
  strip: HUDRect;
  flared: boolean;
  surfaceFlare: number;
  metricsRadius: number;
  targetSize: { width: number; height: number };
  notchGeometry: ReturnType<typeof useNotchGeometry>;
  light: boolean;
  fill: string;
  outline: string;
  /** The ground's shadow, fading with its transparency. */
  shadow: string;
  nativePointer?: unknown;
  entered: (optionDown?: boolean) => void;
  domExited: (event?: { relatedTarget?: EventTarget | null }) => void;
  focusedVendor?: HUDVendor;
  scale: number;
  peekTuck: number;
  pin: (key?: string) => void;
  request?: Approval;
  event?: IslandEvent;
  requests: Approval[];
  snapshot: Snapshot;
  forceOpen: () => void;
  dismissNews: () => void;
  showPanelStats?: () => void;
  configurePanel?: () => void;
  surfacePadding: { top: number; right: number; bottom: number; left: number };
  eventContent: ReactNode;
  inUsagePanel?: boolean;
  delivery?: RequestDelivery;
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
  const from = {
    left: flared && horizontal(edge) ? strip.x - 8 : strip.x,
    top: strip.y,
    width: flared && horizontal(edge) ? strip.width + 16 : strip.width,
    height: strip.height,
    opacity: 0,
  };

  // A card beside the Simple panel is cut where the panel is: the line grows from behind it, and over a see-through
  // look the two would darken each other (Open IslandRootView.outsidePanel). A full card never reaches the panel.
  const cut =
    config.mode === "simple" && detached
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
        {(config.mode === "notch" || cardVisible) && (
          <motion.div
            key={config.mode === "notch" ? "notch-shell" : "hud-card"}
            className={`hud-expanded-surface ${detached ? "hud-detached-card" : ""} ${peek ? "hud-peek" : ""}`}
            initial={
              config.mode === "notch"
                ? false
                : reduceMotion
                  ? false
                  : detached
                    ? {
                        left: frame.x,
                        top: frame.y,
                        width: frame.width,
                        height: frame.height,
                        scaleX: simplePeek && !horizontal(edge) ? 0.2 : 0.86,
                        scaleY: simplePeek && !horizontal(edge) ? 1 : 0.86,
                        opacity: 0,
                      }
                    : from
            }
            animate={
              config.mode === "notch"
                ? { opacity: 1 }
                : {
                    left: frame.x,
                    top: frame.y,
                    width: frame.width,
                    height: frame.height,
                    scaleX: 1,
                    scaleY: 1,
                    opacity: 1,
                  }
            }
            variants={{
              gone: (moving: boolean) =>
                moving
                  ? { opacity: 0, transition: { duration: 0 } }
                  : reduceMotion || detached
                    ? { opacity: 0, scaleX: 0.86, scaleY: 0.86 }
                    : from,
            }}
            exit="gone"
            transition={transition}
            onPointerEnter={(e) => {
              if (!nativePointer) entered(e.altKey);
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
                : {}),
              zIndex: 2,
              pointerEvents: "auto",
              transformOrigin:
                edge === "top"
                  ? "top center"
                  : edge === "bottom"
                    ? "bottom center"
                    : edge === "left"
                      ? "left center"
                      : "right center",
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
              border:
                (detached || config.mode === "simple") && !simplePeek
                  ? `1px solid ${outline}`
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
            {!detached && config.mode === "logos" && (
              <Silhouette
                width={frame.width}
                height={frame.height}
                edge={edge}
                flare={surfaceFlare}
                radius={open ? 26 : 14}
                fill={fill}
                outline={outline}
                duration={reduceMotion || dragging ? 0 : HUD_METRICS.duration}
                initialShape={{
                  width: from.width,
                  height: from.height,
                  flare: 8,
                  radius: metricsRadius,
                }}
              />
            )}
            <AnimatePresence initial={false}>
              {cardVisible && (
                <motion.div
                  key="surface-content"
                  initial={false}
                  animate={{ opacity: 1 }}
                  exit={{
                    opacity: 0,
                    transition: { duration: reduceMotion ? 0 : 0.1 },
                  }}
                  style={{
                    position: "relative",
                    width: "100%",
                    height: "100%",
                    clipPath:
                      config.mode === "notch" ? notchGeometry.clip : undefined,
                  }}
                >
                  <div
                    style={
                      config.mode === "notch"
                        ? {
                            position: "absolute",
                            top: 0,
                            left: "50%",
                            transform: "translateX(-50%)",
                            width: frame.width,
                            height: frame.height,
                          }
                        : { height: "100%" }
                    }
                  >
                    {peek && focusedVendor ? (
                      <button
                        className="hud-peek-hit"
                        onClick={() => pin(focusedVendor.key)}
                        aria-label={`Open ${focusedVendor.name} panel`}
                        style={{
                          position: "relative",
                          border: 0,
                          padding: 0,
                          margin: 0,
                          width: "100%",
                          height: "100%",
                          color: "inherit",
                          background: "transparent",
                          textAlign: "left",
                        }}
                      >
                        <AgentPeek
                          vendor={focusedVendor}
                          simple={config.mode === "simple"}
                          scale={scale}
                          light={light}
                          zh={snapshotLanguage(snapshot) === "zh"}
                          tuck={peekTuck}
                          edge={edge}
                        />
                      </button>
                    ) : !open && (request || event) ? (
                      <CompactAlertView
                        request={request}
                        event={event}
                        requestCount={requests.length}
                        onDock={config.mode !== "notch"}
                        notchWidth={config.notchWidth}
                        surfaceFlare={surfaceFlare}
                        scale={scale}
                        light={rowLight}
                        botLogos={config.botLogos}
                        zh={snapshotLanguage(snapshot) === "zh"}
                        onOpen={() => forceOpen()}
                        onDismissAndStats={() => {
                          dismissNews();
                          showPanelStats?.();
                        }}
                      />
                    ) : (
                      <div
                        style={{
                          position: "relative",
                          maxHeight: "100%",
                          overflowY: "auto",
                          scrollbarWidth: "thin",
                        }}
                      >
                        <motion.div
                          key={
                            request || (event && !inUsagePanel)
                              ? "alert-stack"
                              : (focusedVendor?.key ?? "all")
                          }
                          // Slides in from the edge the HUD sits on.
                          initial={
                            reduceMotion
                              ? false
                              : {
                                  [horizontal(edge) ? "y" : "x"]:
                                    (edge === "top" || edge === "left"
                                      ? -1
                                      : 1) * HUD_METRICS.entryDistance,
                                  opacity: 0,
                                }
                          }
                          animate={{ x: 0, y: 0, opacity: 1 }}
                          transition={{
                            duration: 0.22,
                            delay: 0.12,
                            ease: [0.25, 0.46, 0.45, 0.94],
                          }}
                        >
                          <div
                            className="hud-panel-content"
                            style={{
                              paddingTop: surfacePadding.top,
                              paddingRight: surfacePadding.right,
                              paddingBottom: surfacePadding.bottom,
                              paddingLeft: surfacePadding.left,
                            }}
                          >
                            {eventContent &&
                            (!inUsagePanel || Boolean(request)) ? (
                              eventContent
                            ) : (
                              <SourceHoverPanel
                                active={open}
                                config={config}
                                snapshot={snapshot}
                                focusVendor={focusedVendor?.name}
                                light={light}
                                Logo={HUDLogo}
                                onConfigure={configurePanel}
                                onStats={showPanelStats}
                                inlineAlert={
                                  inUsagePanel && !request
                                    ? eventContent
                                    : undefined
                                }
                                maxHeight={Math.max(
                                  40,
                                  frame.height -
                                    surfacePadding.top -
                                    surfacePadding.bottom,
                                )}
                              />
                            )}
                            <RequestDeliveryNotice
                              delivery={delivery}
                              light={light}
                              language={snapshotLanguage(snapshot)}
                            />
                          </div>
                        </motion.div>
                      </div>
                    )}
                  </div>
                </motion.div>
              )}
            </AnimatePresence>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
