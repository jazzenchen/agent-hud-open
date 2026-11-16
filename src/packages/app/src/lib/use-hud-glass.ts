import { useEffect } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  boxOutline,
  islandOutline,
  type GlassCommand,
} from "@agent-hud/hud/glass";

/** What draws a surface of its own, and so wakes a glass report when it moves
 * or takes glass. By class, not by the glass mark: the notch's surface takes
 * the mark only once it opens, after the observers were bound. */
const SURFACES =
  "[data-hud-glass], .hud-expanded-surface, .hud-mark-strip, .hud-resting-strip, .hud-scene";

type Report = { shapes: GlassCommand[][]; dark: boolean };

function collect(): Report {
  const shapes: GlassCommand[][] = [];
  for (const element of document.querySelectorAll<HTMLElement>(
    "[data-hud-glass]",
  )) {
    const rect = element.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) continue;
    if (element.dataset.hudGlass === "island") {
      shapes.push(
        islandOutline(
          rect.x,
          rect.y,
          rect.width,
          rect.height,
          Number(element.dataset.glassFlare ?? 0),
          Number(element.dataset.glassRadius ?? 0),
        ),
      );
    } else {
      const radius =
        parseFloat(getComputedStyle(element).borderTopLeftRadius) || 0;
      shapes.push(boxOutline(rect.x, rect.y, rect.width, rect.height, radius));
    }
  }
  const dark =
    document.querySelector(".hud-scene")?.getAttribute("data-appearance") !==
    "light";
  return { shapes, dark };
}

/** Lays the system's glass under this HUD window's own surfaces while the
 * look's material is glass: their outlines go to the host every frame one
 * moves, and none once the material is solid again. Only the surfaces wake a
 * report; the animated marks inside them never do. */
export function useHUDGlass(enabled: boolean) {
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let frame = 0;
    let previous = "";
    let pending: Report | null = null;
    let inFlight = false;
    const flush = async () => {
      if (inFlight || pending === null) return;
      const report = pending;
      pending = null;
      inFlight = true;
      try {
        await invoke<boolean>("hud_report_glass", report);
      } catch (error) {
        previous = "";
        if (!disposed) console.error("HUD glass update failed", error);
      } finally {
        inFlight = false;
        if (pending !== null) void flush();
      }
    };
    const send = (report: Report) => {
      const serialized = JSON.stringify(report);
      if (serialized === previous) return;
      previous = serialized;
      pending = report;
      void flush();
    };
    if (!enabled) {
      send({ shapes: [], dark: true });
      return;
    }
    const report = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (!disposed) send(collect());
      });
    };
    const attributes = new MutationObserver(report);
    const bind = () => {
      attributes.disconnect();
      for (const element of document.querySelectorAll(SURFACES))
        attributes.observe(element, {
          attributes: true,
          attributeFilter: [
            "style",
            "class",
            "data-hud-glass",
            "data-glass-flare",
            "data-glass-radius",
            "data-appearance",
          ],
        });
    };
    const relevant = (node: Node) =>
      node instanceof Element &&
      (node.matches(SURFACES) || Boolean(node.querySelector(SURFACES)));
    const tree = new MutationObserver((records) => {
      if (
        records.some((record) =>
          [...record.addedNodes, ...record.removedNodes].some(relevant),
        )
      ) {
        bind();
        report();
      }
    });
    tree.observe(document.body, { childList: true, subtree: true });
    bind();
    window.addEventListener("resize", report);
    report();
    return () => {
      disposed = true;
      attributes.disconnect();
      tree.disconnect();
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", report);
    };
  }, [enabled]);
}
