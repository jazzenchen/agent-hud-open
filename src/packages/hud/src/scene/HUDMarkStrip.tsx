import type { CSSProperties, MutableRefObject } from "react";
import { motion } from "motion/react";
import type { DockMemory, Direction } from "../bots/mark-director";
import { horizontal, type HUDRect, type StripMetrics } from "../geometry";
import {
  HUD_METRICS,
  type HUDConfig,
  type HUDEdge,
  type HUDVendor,
} from "../model";
import { Silhouette } from "./NotchSurface";
import { compactStripMetrics } from "./edge-retract";
import { SIMPLE_EDGE_MARGIN } from "./simple-panel";
import type { HUDNativePointer } from "./useHUDInteraction";
import { useT } from "@agent-hud/ui/format-context";
import { AgentMark } from "./AgentMark";

export function HUDMarkStrip({
  id,
  config,
  edge,
  strip,
  compactStrip,
  retracted = false,
  metrics,
  vendors,
  cardVisible,
  light,
  fill,
  outline,
  dragVisual,
  hovered,
  pointerInside,
  open,
  sender,
  focusedVendor,
  botDirections,
  dockMemory,
  nativePointer,
  reduceMotion,
  hasAlert,
  suppressHover,
  setHovered,
  forceOpen,
  pin,
}: {
  id: string;
  config: HUDConfig;
  edge: HUDEdge;
  strip: HUDRect;
  compactStrip?: HUDRect;
  retracted?: boolean;
  metrics: StripMetrics;
  vendors: HUDVendor[];
  cardVisible: boolean;
  light: boolean;
  fill: string;
  outline: string;
  dragVisual: boolean;
  hovered: string | null;
  pointerInside: boolean;
  open: boolean;
  sender?: { source: string };
  focusedVendor?: HUDVendor;
  botDirections?: Record<string, Direction>;
  dockMemory: MutableRefObject<DockMemory>;
  nativePointer?: HUDNativePointer;
  reduceMotion: boolean;
  hasAlert: boolean;
  suppressHover: MutableRefObject<boolean>;
  setHovered: (key: string | null) => void;
  forceOpen: () => void;
  pin: (key?: string) => void;
}) {
  const t = useT();
  const horiz = horizontal(edge);
  const compact = compactStripMetrics(config, metrics);
  const simpleW = metrics.width - (horiz ? 0 : SIMPLE_EDGE_MARGIN);
  const simpleH = metrics.height - (horiz ? SIMPLE_EDGE_MARGIN : 0);
  const compactW = compactStrip?.width ?? simpleW;
  const compactH = compactStrip?.height ?? simpleH;
  const compactFill = light ? "rgba(245,245,247,0.24)" : "rgba(0,0,0,0.2)";
  const compactOutline = light ? "rgba(0,0,0,0.06)" : "rgba(255,255,255,0.08)";
  const anchorStyle: CSSProperties =
    edge === "left"
      ? { position: "absolute", left: 0, top: "50%" }
      : edge === "right"
        ? { position: "absolute", right: 0, top: "50%" }
        : edge === "top"
          ? { position: "absolute", top: 0, left: "50%" }
          : { position: "absolute", bottom: 0, left: "50%" };

  return (
    <>
      {config.mode === "notch" ? (
        <div
          className="hud-classic-silhouette"
          aria-label={t("Hover to expand Agent HUD")}
          tabIndex={0}
          role="button"
          aria-describedby={`hud-instructions-${id}`}
          onFocus={() => {
            if (!hasAlert && !suppressHover.current) forceOpen();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") forceOpen();
          }}
          style={{
            position: "absolute",
            left: -8,
            top: 0,
            width: strip.width + 16,
            height: strip.height,
            cursor: dragVisual ? "grabbing" : "default",
          }}
        >
          {config.hasNotch && cardVisible && light && (
            <Silhouette
              width={strip.width + 16}
              height={strip.height}
              edge="top"
              flare={8}
              radius={metrics.radius}
              fill="#000"
              outline={outline}
            />
          )}
        </div>
      ) : (
        <motion.div
          className="hud-mark-strip"
          initial={false}
          animate={{
            x: horiz ? "-50%" : 0,
            y: horiz ? 0 : "-50%",
            width: retracted ? compactW : simpleW,
            height: retracted ? compactH : simpleH,
            marginTop: !retracted && edge === "top" ? SIMPLE_EDGE_MARGIN : 0,
            marginRight:
              !retracted && edge === "right" ? SIMPLE_EDGE_MARGIN : 0,
            marginBottom:
              !retracted && edge === "bottom" ? SIMPLE_EDGE_MARGIN : 0,
            marginLeft: !retracted && edge === "left" ? SIMPLE_EDGE_MARGIN : 0,
            paddingTop: retracted ? compact.pad : metrics.padding.top,
            paddingRight: retracted ? compact.pad : metrics.padding.right,
            paddingBottom: retracted ? compact.pad : metrics.padding.bottom,
            paddingLeft: retracted ? compact.pad : metrics.padding.left,
            gap: `${retracted ? compact.gap : metrics.gap}px`,
            borderRadius: retracted ? compact.radius : metrics.radius,
            backgroundColor: retracted ? compactFill : fill,
            boxShadow: `inset 0 0 0 1px ${retracted ? compactOutline : outline}`,
          }}
          transition={{
            duration: reduceMotion ? 0 : 0.24,
            ease: HUD_METRICS.ease,
          }}
          style={{
            ...anchorStyle,
            boxSizing: "border-box",
            display: "flex",
            flexDirection: horiz ? "row" : "column",
            alignItems: horiz ? "center" : "flex-start",
            justifyContent: "flex-start",
            overflow: "visible",
            border: 0,
            pointerEvents: retracted ? "none" : undefined,
          }}
        >
          {vendors.map((vendor, index) => (
            <AgentMark
              key={vendor.key}
              vendor={vendor}
              index={index}
              config={config}
              edge={edge}
              strip={strip}
              metrics={metrics}
              compact={retracted}
              compactSize={compact.mark}
              light={light}
              dragVisual={dragVisual}
              hovered={hovered}
              pointerInside={pointerInside}
              open={open}
              sender={sender}
              focusedVendor={focusedVendor}
              botDirections={botDirections}
              dockMemory={dockMemory}
              nativePointer={nativePointer}
              reduceMotion={reduceMotion}
              hasAlert={hasAlert}
              suppressHover={suppressHover}
              setHovered={setHovered}
              forceOpen={forceOpen}
              pin={pin}
            />
          ))}
        </motion.div>
      )}
    </>
  );
}
