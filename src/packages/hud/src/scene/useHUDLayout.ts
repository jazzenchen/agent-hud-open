import { useMemo, type CSSProperties } from "react";
import type {
  Approval,
  IslandEvent,
  RequestDelivery,
  Snapshot,
} from "@agent-hud/protocol";
import { compactAlertDockWidth } from "../IslandNews";
import {
  cardRect,
  dragSurfaceGeometry,
  horizontal,
  parkedRect,
  type HUDRect,
  type StripMetrics,
} from "../geometry";
import {
  HUD_METRICS,
  vendorKey,
  type HUDConfig,
  type HUDEdge,
  type HUDVendor,
} from "../model";
import { peekLineWidth } from "./AgentPeek";
import { SIMPLE_EDGE_MARGIN } from "./simple-panel";
import { useNotchGeometry } from "./NotchSurface";
import { snapshotLanguage } from "@agent-hud/ui/format";

export interface UseHUDLayoutArgs {
  config: HUDConfig;
  snapshot: Snapshot;
  vendors: HUDVendor[];
  metrics: StripMetrics;
  screen: { width: number; height: number };
  notchFrame?: HUDRect;
  contentHeight: number;
  systemLight: boolean;
  open: boolean;
  pinned: boolean;
  dragging: boolean;
  reduceMotion: boolean;
  hovered: string | null;
  request?: Approval;
  event?: IslandEvent;
  inUsagePanel?: boolean;
  requestsCount: number;
}

export function useHUDLayout({
  config,
  snapshot,
  vendors,
  metrics,
  screen,
  notchFrame,
  contentHeight,
  systemLight,
  open,
  pinned,
  dragging,
  reduceMotion,
  hovered,
  request,
  event,
  inUsagePanel = false,
  requestsCount,
}: UseHUDLayoutArgs) {
  const edge: HUDEdge = config.mode === "notch" ? "top" : config.edge;
  const strip =
    config.mode === "notch" && notchFrame
      ? notchFrame
      : parkedRect(screen, metrics, config);
  const grab = dragSurfaceGeometry(strip, config, metrics.radius, screen);
  const light =
    config.appearance === "light" ||
    (config.appearance === "system" && systemLight);
  // Transparency runs the ground from opaque to fully clear. Its outline and
  // shadow belong to the ground and fade with it; logos, bots and text are
  // drawn over it at full strength.
  const surfaceAlpha = 1 - config.solidTransparency;
  const fill = light
    ? `rgba(245,245,247,${surfaceAlpha})`
    : `rgba(0,0,0,${surfaceAlpha})`;
  const outline = light
    ? `rgba(0,0,0,${0.106 * surfaceAlpha})`
    : `rgba(255,255,255,${0.075 * surfaceAlpha})`;
  const shadow = `0 9px 30px rgba(0,0,0,${0.133 * surfaceAlpha})`;

  // The agent the card is for: the sender of the event on show, otherwise the hovered mark. A dock never gives
  // its marks up to an event, so hovering another mark does not move an event's card (ScreenHUD.focus).
  const sender = request ?? event;
  const standaloneAlert = Boolean(request || (event && !inUsagePanel));
  const focusedVendor =
    sender && config.mode !== "notch" && !inUsagePanel
      ? vendors.find((v) => v.key === vendorKey(sender.source))
      : config.separatesAgents && config.mode !== "notch"
        ? vendors.find((v) => v.key === hovered)
        : undefined;

  const delivery = useMemo(() => {
    const now = Date.parse(snapshot.generatedAt);
    const source = request?.source ?? focusedVendor?.key;
    let latest: RequestDelivery | undefined;
    let latestAt = -Infinity;
    for (const entry of snapshot.requestDeliveries ?? []) {
      const at = Date.parse(entry.at);
      if (entry.state === "confirmed" || now - at > 300_000) continue;
      if (source && vendorKey(entry.source) !== vendorKey(source)) continue;
      if (request?.sessionId && entry.sessionId !== request.sessionId) continue;
      if (at > latestAt) {
        latest = entry;
        latestAt = at;
      }
    }
    return latest;
  }, [
    snapshot.generatedAt,
    snapshot.requestDeliveries,
    request?.source,
    request?.sessionId,
    focusedVendor?.key,
  ]);

  const detached =
    config.mode !== "notch" && Boolean(focusedVendor || standaloneAlert);
  const peek = Boolean(
    open && detached && !pinned && !standaloneAlert && focusedVendor,
  );
  const scale = Math.max(
    1,
    (config.mode === "simple"
      ? config.simpleSize
      : config.mode === "logos"
        ? config.logoSize
        : 26) / 26,
  );
  const simplePeek = Boolean(peek && config.mode === "simple");
  const surfaceFlare =
    open || config.mode === "logos"
      ? HUD_METRICS.expandedFlare
      : HUD_METRICS.collapsedFlare;
  const flared = !detached && config.mode !== "simple";
  const fullWidth = standaloneAlert
    ? 470
    : focusedVendor
      ? Math.max(440, config.panelWidth - 100)
      : !horizontal(edge) && !detached
        ? Math.min(
            screen.width - 16,
            Math.max(
              config.panelWidth,
              404 + metrics.width + 12 + surfaceFlare + 16,
            ),
          )
        : config.panelWidth;
  // On a side edge the line starts behind the dock, so it reads as the dock growing; that side carries no text.
  const peekTuck =
    simplePeek && !horizontal(edge) ? Math.round(strip.width / 2) : 0;
  const peekWidth =
    simplePeek && focusedVendor
      ? peekLineWidth(
          focusedVendor,
          scale,
          snapshotLanguage(snapshot) === "zh",
          peekTuck,
          snapshot.settings.quotaDisplay,
        )
      : 196 * scale;
  const targetSize =
    !open && (request || event)
      ? {
          width:
            config.mode === "notch"
              ? config.notchWidth + 256
              : Math.min(
                  compactAlertDockWidth(
                    request,
                    event,
                    requestsCount,
                    scale,
                    snapshotLanguage(snapshot) === "zh",
                  ),
                  screen.width,
                ),
          height:
            config.mode === "notch"
              ? Math.max(38, config.notchHeight)
              : 26 * scale,
        }
      : peek && focusedVendor
        ? {
            width: peekWidth,
            height: simplePeek
              ? 26 * scale
              : (20 +
                  (1 +
                    focusedVendor.quotas.length +
                    (focusedVendor.balance ? 1 : 0)) *
                    18) *
                scale,
          }
        : {
            width: fullWidth,
            height: Math.max(80, Math.min(contentHeight, screen.height - 80)),
          };

  const focusedIndex = vendors.findIndex((v) => v.key === focusedVendor?.key);
  // ScreenHUD.eventMark: when an alert's vendor isn't on the dock strip, anchor to the middle of the strip.
  const anchor =
    focusedIndex >= 0
      ? (horizontal(edge) ? strip.x : strip.y) + metrics.centers[focusedIndex]
      : sender && config.mode !== "notch" && !inUsagePanel
        ? horizontal(edge)
          ? strip.x + strip.width / 2
          : strip.y + strip.height / 2
        : undefined;
  const canvasSize =
    config.mode === "logos" && flared
      ? {
          width: targetSize.width + (horizontal(edge) ? surfaceFlare * 2 : 0),
          height: targetSize.height + (horizontal(edge) ? 0 : surfaceFlare * 2),
        }
      : targetSize;
  const placed = cardRect(
    screen,
    strip,
    edge,
    canvasSize,
    detached,
    anchor,
    simplePeek && !horizontal(edge) ? -strip.width / 2 : 6,
  );
  const frame =
    config.mode === "notch"
      ? {
          ...placed,
          x: placed.x - surfaceFlare,
          width: placed.width + surfaceFlare * 2,
        }
      : placed;
  const cardVisible = !dragging && (open || Boolean(request || event));
  const collapsedCanvas = { ...strip, x: strip.x - 8, width: strip.width + 16 };
  const notchTarget = cardVisible ? frame : collapsedCanvas;
  const notchRadius = cardVisible
    ? open
      ? 26
      : Math.max(metrics.radius, 14)
    : metrics.radius;
  const notchGeometry = useNotchGeometry(
    notchTarget,
    cardVisible ? surfaceFlare : 8,
    notchRadius,
    reduceMotion || dragging,
  );

  const bodyPadding = detached
    ? { top: 14, right: 18, bottom: 14, left: 18 }
    : config.mode === "notch"
      ? {
          top: 32 + (config.hasNotch && light ? 14 : 0),
          right: 18,
          bottom: 14,
          left: 18,
        }
      : {
          top: edge === "top" ? metrics.height + 12 : 14,
          right: edge === "right" ? metrics.width + 12 : 18,
          bottom: edge === "bottom" ? metrics.height + 12 : 14,
          left: edge === "left" ? metrics.width + 12 : 18,
        };
  const surfacePadding = {
    top: bodyPadding.top + (flared && !horizontal(edge) ? surfaceFlare : 0),
    right: bodyPadding.right + (flared && horizontal(edge) ? surfaceFlare : 0),
    bottom:
      bodyPadding.bottom + (flared && !horizontal(edge) ? surfaceFlare : 0),
    left: bodyPadding.left + (flared && horizontal(edge) ? surfaceFlare : 0),
  };
  const marginStyle: CSSProperties =
    config.mode !== "simple"
      ? {}
      : edge === "top"
        ? { marginTop: SIMPLE_EDGE_MARGIN }
        : edge === "bottom"
          ? { marginBottom: SIMPLE_EDGE_MARGIN }
          : edge === "left"
            ? { marginLeft: SIMPLE_EDGE_MARGIN }
            : { marginRight: SIMPLE_EDGE_MARGIN };
  const measurementWidth = Math.max(
    1,
    Math.min(
      fullWidth,
      screen.width -
        (config.mode === "logos" && !detached && horizontal(edge)
          ? HUD_METRICS.expandedFlare * 2
          : 0),
    ),
  );

  return {
    edge,
    strip,
    grab,
    light,
    fill,
    outline,
    shadow,
    sender,
    focusedVendor,
    delivery,
    detached,
    peek,
    scale,
    simplePeek,
    peekTuck,
    targetSize,
    surfaceFlare,
    flared,
    frame,
    cardVisible,
    notchGeometry,
    bodyPadding,
    surfacePadding,
    marginStyle,
    measurementWidth,
  };
}
