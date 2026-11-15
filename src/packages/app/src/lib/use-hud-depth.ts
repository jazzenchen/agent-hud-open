import { useEffect } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { HUDEdge } from "@agent-hud/hud/model";

/** What the HUD shows at rest: its strip, the cards out beside it, a bot's
 * bubble, a paper plane in flight. */
const PARTS =
  ".hud-resting-strip, .hud-expanded-surface, .hud-grab-surface, .hud-hover-floor, .hud-dock-end, .hud-guide, .hud-agent-peek-bubble, .hud-flight";
/** Room beyond what is measured, for the glow, shadows and a card coming in. */
const MARGIN = 72;
/** A growing window rounds up to this, so a card animating in asks once or twice. */
const STEP = 96;
/** How long a collapse plays before the window shrinks behind it. */
const SETTLE_MS = 900;

/** How far the HUD reaches from its edge, in points, inside `scene`. */
export function reachFromEdge(
  edge: HUDEdge,
  scene: DOMRect,
  parts: Iterable<DOMRect>,
): number {
  let reach = 0;
  for (const part of parts) {
    if (part.width <= 0 || part.height <= 0) continue;
    const from =
      edge === "top"
        ? part.bottom - scene.top
        : edge === "bottom"
          ? scene.bottom - part.top
          : edge === "left"
            ? part.right - scene.left
            : scene.right - part.left;
    reach = Math.max(reach, from);
  }
  return reach;
}

/** The window depth to ask for: null (the whole canvas) while the panel opens
 * or is open, otherwise what the HUD reaches plus room, in whole steps. */
export function wantedDepth(open: boolean, reach: number): number | null {
  return open ? null : Math.ceil((reach + MARGIN) / STEP) * STEP;
}

/** Keeps this HUD's window only as deep as the HUD needs (agent-hud-open #84).
 * It grows at once, to the whole canvas the moment the pointer starts to open
 * the panel, and shrinks only after a collapse has played. */
export function useHUDDepth(edge: HUDEdge | undefined, enabled: boolean) {
  useEffect(() => {
    if (!enabled || !edge || !isTauri()) return;
    let asked: number | null | undefined;
    let shrink: ReturnType<typeof setTimeout> | null = null;
    let frame = 0;
    let disposed = false;
    const send = (depth: number | null) => {
      if (depth === asked) return;
      asked = depth;
      invoke("hud_set_depth", { depth }).catch((error) => {
        asked = undefined;
        if (!disposed) console.error("HUD depth update failed", error);
      });
    };
    const measure = (settled: boolean) => {
      const scene = document.querySelector<HTMLElement>(".hud-scene");
      if (!scene) return;
      const open =
        scene.dataset.hover !== "collapsed" ||
        scene.dataset.open === "true" ||
        scene.dataset.pinned === "true";
      const reach = reachFromEdge(
        edge,
        scene.getBoundingClientRect(),
        [...scene.querySelectorAll(PARTS)].map((part) =>
          part.getBoundingClientRect(),
        ),
      );
      const depth = wantedDepth(open, reach);
      const deeper =
        asked === undefined ||
        depth === null ||
        (asked !== null && depth > asked);
      if (deeper || settled) {
        if (shrink) clearTimeout(shrink);
        shrink = null;
        send(depth);
      } else if (depth !== asked && !shrink) {
        shrink = setTimeout(() => {
          shrink = null;
          if (!disposed) measure(true);
        }, SETTLE_MS);
      }
    };
    const queue = () => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        if (!disposed) measure(false);
      });
    };
    const observer = new MutationObserver(queue);
    const root = document.getElementById("root");
    if (root)
      observer.observe(root, {
        subtree: true,
        childList: true,
        attributes: true,
        attributeFilter: [
          "style",
          "class",
          "data-hover",
          "data-open",
          "data-pinned",
        ],
      });
    queue();
    return () => {
      disposed = true;
      observer.disconnect();
      cancelAnimationFrame(frame);
      if (shrink) clearTimeout(shrink);
    };
  }, [edge, enabled]);
}
