import type { MutableRefObject } from "react";
import { quotaDisplayLabel } from "@agent-hud/ui/format";
import { useFormat } from "@agent-hud/ui/format-context";
import { BotMark } from "../BotMark";
import { botState, facingFromEdge, isAttention } from "../bots/bot-state";
import type { DockMemory, Direction } from "../bots/mark-director";
import { horizontal, type HUDRect, type StripMetrics } from "../geometry";
import { useMarkLevelText } from "../ground";
import {
  markUsed,
  meterLabel,
  simpleNumberColor,
  vendorKey,
  type HUDConfig,
  type HUDEdge,
  type HUDVendor,
} from "../model";
import { isRingStyle } from "../usage-meter/arcs";
import { MarkBars, MarkRing, markItems } from "../usage-meter/mark-usage";
import { compactMark } from "./rest-style";
import type { HUDNativePointer } from "./useHUDInteraction";

const clampedSize = (size: number) => Math.max(20, Math.min(60, size));

/** One agent's mark on a dock: its bot or logo, with its usage as rings,
 * bars or a number in Simple mode, and a dot while it waits for an answer.
 * On a dock that rests as its logos, `compact` folds it down to its logo. */
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
  setHovered,
  pin,
  compact,
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
  setHovered: (key: string | null) => void;
  pin: (key?: string) => void;
  /** Folded to its logo; undefined where the dock never rests as its logos. */
  compact?: boolean;
}) {
  const format = useFormat();
  const { quotaDisplay } = format;
  // A ring's figure is text: on a see-through ground, its level's see-through shade.
  const figureColor = useMarkLevelText(light);
  const parts =
    compact === undefined ? null : compactMark(compact, metrics, reduceMotion);
  const bars = config.mode === "simple" && metrics.bar > 0 && (
    <MarkBars
      items={markItems(vendor, quotaDisplay, config.panelLayout)}
      length={metrics.bar}
      height={metrics.mark}
      size={clampedSize(config.simpleSize)}
      ringWidth={config.ringWidth}
      light={light}
      reduceMotion={reduceMotion}
    />
  );
  const figure = config.mode === "simple" &&
    (config.showsNumbers || config.simpleStyle === "numbers") && (
      <span
        style={{
          color: simpleNumberColor(config, light, figureColor(vendor.level)),
          fontSize: metrics.font,
          fontWeight: 400,
          fontVariantNumeric: "tabular-nums",
          whiteSpace: "nowrap",
          textAlign: "right",
          minWidth: horizontal(edge) ? undefined : metrics.label,
        }}
      >
        {meterLabel(vendor, quotaDisplay)}
      </span>
    );
  return (
    <button
      type="button"
      className="hud-agent-mark"
      aria-label={`${vendor.name}, ${meterLabel(vendor, quotaDisplay)} ${markUsed(vendor) == null ? "" : quotaDisplayLabel(quotaDisplay, format.language)}`}
      onPointerEnter={() => {
        if (hovered !== vendor.key) {
          setHovered(vendor.key);
        }
      }}
      // The HUD takes keys only for a text field, so focus here never comes
      // from the person: let it go, with no ring and nothing opened.
      tabIndex={-1}
      onFocus={(event) => event.currentTarget.blur()}
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
        ...parts?.button,
      }}
    >
      <span
        style={{
          width: metrics.mark,
          height: metrics.mark,
          display: "inline-grid",
          placeItems: "center",
          position: "relative",
          ...parts?.mark,
        }}
      >
        {config.mode === "simple" && isRingStyle(config.simpleStyle) && (
          <MarkRing
            style={config.simpleStyle}
            items={markItems(vendor, quotaDisplay, config.panelLayout)}
            size={metrics.mark}
            ringWidth={config.ringWidth}
            light={light}
            reduceMotion={reduceMotion}
            styles={parts?.ring}
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
        {vendor.waiting > 0 && (
          <span
            aria-hidden="true"
            style={{
              position: "absolute",
              right: -2,
              bottom: -2,
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: "#e9a16d",
              boxShadow: `0 0 0 2px ${light ? "#f8f8fa" : "#000"}`,
            }}
          />
        )}
      </span>
      {parts
        ? (bars || figure) && (
            <span aria-hidden={compact || undefined} style={parts.figures}>
              {bars}
              {figure}
            </span>
          )
        : bars}
      {!parts && figure}
    </button>
  );
}
