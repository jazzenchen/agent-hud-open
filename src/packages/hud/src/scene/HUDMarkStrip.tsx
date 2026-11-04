import type { CSSProperties, MutableRefObject } from "react";
import type { GlowAlert } from "../HUDGlow";
import type { DockMemory, Direction } from "../bots/mark-director";
import { horizontal, type HUDRect, type stripMetrics } from "../geometry";
import type { HUDConfig, HUDEdge, HUDVendor } from "../model";
import { Silhouette } from "./NotchSurface";
import type { HUDNativePointer } from "./useHUDInteraction";
import { useFormat } from "@agent-hud/ui/format-context";
import { AgentMark } from "./AgentMark";
import { DockGlowBackdrop } from "./DockGlowBackdrop";

export function HUDMarkStrip({
  id,
  config,
  edge,
  strip,
  metrics,
  vendors,
  glowColors,
  glowAlert,
  glowHidden,
  cardVisible,
  detached,
  light,
  fill,
  outline,
  marginStyle,
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
  metrics: ReturnType<typeof stripMetrics>;
  vendors: HUDVendor[];
  glowColors: string[];
  glowAlert?: GlowAlert | null;
  glowHidden: boolean;
  cardVisible: boolean;
  detached: boolean;
  light: boolean;
  fill: string;
  outline: string;
  marginStyle: CSSProperties;
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
  const format = useFormat();
  const zh = format.language === "zh";

  return (
    <>
      {config.mode === "logos" && !glowHidden && (!cardVisible || detached) && (
        <DockGlowBackdrop
          config={config}
          edge={edge}
          strip={strip}
          vendors={vendors}
          glowColors={glowColors}
          glowAlert={glowAlert}
          reduceMotion={reduceMotion}
        />
      )}
      {config.mode === "notch" ? (
        <div
          className="hud-classic-silhouette"
          aria-label={zh ? "悬停展开 Agent HUD" : "Hover to expand Agent HUD"}
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
        <div
          className="hud-mark-strip"
          style={{
            position: "relative",
            display: "flex",
            flexDirection: horizontal(edge) ? "row" : "column",
            alignItems: horizontal(edge) ? "center" : "flex-start",
            justifyContent: "flex-start",
            gap: metrics.gap,
            ...(config.mode === "simple"
              ? {
                  ...marginStyle,
                  width: metrics.width - (horizontal(edge) ? 0 : 3),
                  height: metrics.height - (horizontal(edge) ? 3 : 0),
                  paddingTop: metrics.padding.top,
                  paddingRight: metrics.padding.right,
                  paddingBottom: metrics.padding.bottom,
                  paddingLeft: metrics.padding.left,
                  borderRadius: metrics.radius,
                  // The dock keeps its own ground while a panel or card is
                  // open; what opens is cut where the dock is.
                  background: fill,
                  border: 0,
                  boxShadow: `inset 0 0 0 1px ${outline}`,
                  backdropFilter: undefined,
                }
              : {
                  width: strip.width,
                  height: strip.height,
                  alignItems: "center",
                  justifyContent: "center",
                }),
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
        </div>
      )}
    </>
  );
}
