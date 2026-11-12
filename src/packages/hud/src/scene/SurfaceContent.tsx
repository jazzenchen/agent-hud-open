import type { ReactNode } from "react";
import { motion } from "motion/react";
import type { Approval, IslandEvent, Snapshot } from "@agent-hud/protocol";
import { snapshotLanguage } from "@agent-hud/ui/format";
import { CompactAlertView } from "../alerts/CompactAlertView";
import { horizontal, type HUDRect } from "../geometry";
import {
  HUD_METRICS,
  type HUDConfig,
  type HUDEdge,
  type HUDVendor,
} from "../model";
import { AgentPeek } from "./AgentPeek";

export interface SurfaceContentProps {
  config: HUDConfig;
  edge: HUDEdge;
  frame: HUDRect;
  peek: boolean;
  open: boolean;
  reduceMotion: boolean;
  surfaceFlare: number;
  light: boolean;
  /** The compact alert's look: a hardware notch's row stays dark. */
  rowLight: boolean;
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
  surfacePadding: { top: number; right: number; bottom: number; left: number };
  inUsagePanel?: boolean;
  /** The panel's body: an alert stack or the usage panel. */
  children: ReactNode;
}

/** What the open surface shows: an agent's peek, a compact alert while the
 * panel is closed, or the panel's body sliding in from the HUD's edge. */
export function SurfaceContent({
  config,
  edge,
  frame,
  peek,
  open,
  reduceMotion,
  surfaceFlare,
  light,
  rowLight,
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
  surfacePadding,
  inUsagePanel,
  children,
}: SurfaceContentProps) {
  return (
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
          className="hud-bare-scroll"
          style={{
            position: "relative",
            maxHeight: "100%",
            overflowX: "hidden",
            overflowY: "auto",
            scrollbarWidth: "none",
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
                      (edge === "top" || edge === "left" ? -1 : 1) *
                      HUD_METRICS.entryDistance,
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
              {children}
            </div>
          </motion.div>
        </div>
      )}
    </div>
  );
}
