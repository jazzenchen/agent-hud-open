import type { MutableRefObject } from "react";
import { quotaDisplayLabel, quotaPercentLabel } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { BotMark } from "../BotMark";
import { botState, facingFromEdge, isAttention } from "../bots/bot-state";
import type { DockMemory, Direction } from "../bots/mark-director";
import { horizontal, type HUDRect, type StripMetrics } from "../geometry";
import { useMarkLevelText } from "../ground";
import {
  markUsed,
  meterLabel,
  dockNumberColors,
  vendorKey,
  type HUDConfig,
  type HUDEdge,
  type HUDVendor,
} from "../model";
import { isRingStyle } from "../usage-meter/arcs";
import { useDockQuota } from "./useDockQuota";
import { DockQuotaBadge } from "./DockQuotaBadge";
import { MarkBars, MarkRing, markItems } from "../usage-meter/mark-usage";
import type { HUDNativePointer } from "./useHUDInteraction";
import { compactLabel } from "../compact-dock";
import { DOCK_SCALE, dockFontWeight, dockScale } from "../dock-scale";

const clampedSize = (size: number) => Math.max(20, Math.min(60, size));

/** One agent's mark on a dock: its bot or logo, with its usage as rings,
 * bars or a number in Simple mode, and a dot while it waits for an answer. */
export function AgentMark({
  vendor,
  index,
  config,
  edge,
  strip,
  metrics,
  light,
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
  vendor: HUDVendor;
  index: number;
  config: HUDConfig;
  edge: HUDEdge;
  strip: HUDRect;
  metrics: StripMetrics;
  light: boolean;
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
  const { quotaDisplay } = format;
  const compact = config.mode === "simple" && config.compactDock;
  const scale = DOCK_SCALE * dockScale(config.dockScale);
  const { quota, alert } = useDockQuota(
    vendor.quotas,
    config.dockQuotaWarningThreshold,
    config.mode === "simple",
  );
  const label = quota
    ? quotaPercentLabel(quota.usedPercent, quotaDisplay)
    : meterLabel(vendor, quotaDisplay);
  const description = [
    vendor.name,
    quota?.name,
    markUsed(vendor) == null
      ? label
      : `${label} ${quotaDisplayLabel(quotaDisplay, format.language)}`,
  ]
    .filter(Boolean)
    .join(" · ");
  const caption = compact ? compactLabel(vendor, label) : label;
  // A ring's figure is text: on a see-through ground, its level's see-through shade.
  const figureColor = useMarkLevelText(light);
  return (
    <button
      type="button"
      className="hud-agent-mark"
      title={description}
      aria-label={description}
      onPointerEnter={() => {
        if (hovered !== vendor.key) {
          setHovered(vendor.key);
        }
      }}
      onFocus={() => {
        setHovered(vendor.key);
        if (!hasAlert && !suppressHover.current) forceOpen();
      }}
      onClick={() => pin(vendor.key)}
      style={{
        position: "relative",
        zIndex: 1,
        border: 0,
        padding: 0,
        color: "inherit",
        background: "transparent",
        display: "flex",
        alignItems: "center",
        gap: metrics.labelGap,
        flexShrink: 0,
        cursor: dragVisual
          ? "grabbing"
          : config.separatesAgents
            ? "pointer"
            : "default",
      }}
    >
      <span
        style={{
          width: metrics.mark,
          height: metrics.mark,
          display: "inline-grid",
          placeItems: "center",
          position: "relative",
        }}
      >
        {config.mode === "simple" &&
          !compact &&
          isRingStyle(config.simpleStyle) && (
            <MarkRing
              style={config.simpleStyle}
              items={markItems(vendor, quotaDisplay, config.panelLayout)}
              size={metrics.mark}
              ringWidth={config.ringWidth * scale}
              light={light}
              reduceMotion={reduceMotion}
            />
          )}
        <BotMark
          source={vendor.name}
          size={metrics.logo}
          light={light}
          enabled={config.botLogos}
          hovered={
            ((hovered === vendor.key && pointerInside) ||
              (open &&
                !sender &&
                (!focusedVendor || focusedVendor.key === vendor.key))) &&
            !dragVisual
          }
          state={(() => {
            const state =
              botDirections?.[vendor.key]?.state ??
              botState({
                look:
                  vendor.waiting > 0
                    ? "permission"
                    : vendor.working > 0
                      ? "plain"
                      : "idle",
              });
            const facesCard = sender
              ? vendorKey(sender.source) === vendor.key
              : open && (!focusedVendor || focusedVendor.key === vendor.key);
            return {
              ...state,
              facing:
                isAttention(state.look) && facesCard
                  ? facingFromEdge(edge)
                  : null,
            };
          })()}
          cue={(() => {
            const direction = botDirections?.[vendor.key];
            return direction
              ? dockMemory.current.cue(
                  vendor.key,
                  direction.cue,
                  direction.state.badge,
                  performance.now() / 1000,
                )
              : null;
          })()}
          staysBot={false}
          aim={
            nativePointer?.point
              ? (() => {
                  const centerX =
                    strip.x +
                    (horizontal(edge)
                      ? metrics.centers[index]
                      : metrics.padding.left + metrics.mark / 2);
                  const centerY =
                    strip.y +
                    (horizontal(edge)
                      ? metrics.padding.top + metrics.mark / 2
                      : metrics.centers[index]);
                  const dx = nativePointer.point!.x - centerX,
                    dy = nativePointer.point!.y - centerY,
                    distance = Math.hypot(dx, dy);
                  return distance < 1
                    ? ([0, 0] as const)
                    : ([dx / (distance + 120), dy / (distance + 120)] as const);
                })()
              : undefined
          }
        />
        {alert && <DockQuotaBadge description={description} scale={scale} />}
        {vendor.waiting > 0 && (
          <span
            aria-hidden="true"
            style={{
              position: "absolute",
              right: -2 * scale,
              bottom: -2 * scale,
              width: 8 * scale,
              height: 8 * scale,
              borderRadius: "50%",
              background: "#e9a16d",
              boxShadow: `0 0 0 ${2 * scale}px ${light ? "#f8f8fa" : "#000"}`,
            }}
          />
        )}
      </span>
      {config.mode === "simple" && !compact && metrics.bar > 0 && (
        <MarkBars
          items={markItems(vendor, quotaDisplay, config.panelLayout)}
          length={metrics.bar}
          height={metrics.mark}
          size={clampedSize(config.simpleSize) * scale}
          ringWidth={config.ringWidth * scale}
          light={light}
          reduceMotion={reduceMotion}
        />
      )}
      {config.mode === "simple" &&
        (compact ||
          config.showsNumbers ||
          config.simpleStyle === "numbers") && (
          <span
            style={{
              ...dockNumberColors(
                config.dockNumberStyle,
                config.dockNumberTransparency,
                light,
                figureColor(quota?.status ?? vendor.level),
                Boolean(alert),
              ),
              borderRadius: 4 * scale,
              fontSize: metrics.font,
              fontWeight: dockFontWeight(config.dockScale),
              fontVariantNumeric: "tabular-nums",
              whiteSpace: "nowrap",
              textAlign: "right",
              minWidth: horizontal(edge)
                ? metrics.labels?.[index]
                : metrics.label,
            }}
          >
            {caption}
          </span>
        )}
    </button>
  );
}
