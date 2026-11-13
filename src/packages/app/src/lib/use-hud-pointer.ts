import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { HUDNativePointer } from "@agent-hud/hud/HUDScene";

type NativePointerEvent = HUDNativePointer & { screenId: string };
const outsidePointer: HUDNativePointer = {
  inside: false,
  point: null,
  buttons: null,
  altKey: null,
  ctrlKey: null,
  metaKey: null,
  shiftKey: null,
};
/** The pointer on the guide's window beside the HUD is not on the HUD: it
 * clicks the window's buttons but never opens the panel. */
function offGuide<T extends HUDNativePointer>(pointer: T): T {
  const guide = document.querySelector(".hud-guide")?.getBoundingClientRect();
  const point = pointer.point;
  if (!pointer.inside || !point || !guide?.width) return pointer;
  const scene = document.querySelector(".hud-scene")?.getBoundingClientRect();
  const x = point.x + (scene?.left ?? 0);
  const y = point.y + (scene?.top ?? 0);
  const over =
    x >= guide.left && x <= guide.right && y >= guide.top && y <= guide.bottom;
  return over ? { ...pointer, inside: false } : pointer;
}
/** Only outer HUD geometry wakes native reports; animated bot descendants never do. */
function observeHUDContours(report: () => void) {
  const selector =
    ".hud-scene, .hud-resting-strip, .hud-expanded-surface, .hud-grab-surface, .hud-hover-floor, .hud-dock-end, .hud-guide";
  let timer: ReturnType<typeof setTimeout> | null = null,
    last = 0;
  const queue = () => {
    if (timer !== null) return;
    timer = setTimeout(
      () => {
        timer = null;
        last = performance.now();
        report();
      },
      Math.max(0, 34 - (performance.now() - last)),
    );
  };
  const attributes = new MutationObserver(queue);
  const bind = () => {
    attributes.disconnect();
    for (const element of document.querySelectorAll(selector))
      attributes.observe(element, {
        attributes: true,
        attributeFilter: ["style", "class"],
      });
  };
  const relevant = (node: Node) =>
    node instanceof Element &&
    (node.matches(selector) || Boolean(node.querySelector(selector)));
  const tree = new MutationObserver((records) => {
    if (
      records.some((record) =>
        [...record.addedNodes, ...record.removedNodes].some(relevant),
      )
    ) {
      bind();
      queue();
    }
  });
  const root = document.getElementById("root");
  if (root) tree.observe(root, { subtree: true, childList: true });
  bind();
  return () => {
    attributes.disconnect();
    tree.disconnect();
    if (timer !== null) clearTimeout(timer);
  };
}

/** The native pointer over this screen's HUD. The HUD reports its hit regions
 * to the host, which answers each report with where the pointer is, and sends
 * every move after. */
export function useHUDPointer(screenId: string) {
  const [nativePointer, setNativePointer] = useState<
    HUDNativePointer | undefined
  >(() => (isTauri() ? outsidePointer : undefined));
  const pointerRevision = useRef(0);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    setNativePointer(outsidePointer);
    const pointerSubscription = listen<NativePointerEvent>(
      "hud:pointer",
      (event) => {
        if (disposed || event.payload.screenId !== screenId) return;
        pointerRevision.current += 1;
        setNativePointer(offGuide(event.payload));
      },
    );
    return () => {
      disposed = true;
      void pointerSubscription.then((unlisten) => unlisten());
    };
  }, [screenId]);
  useEffect(() => {
    if (!isTauri()) return;
    let frame = 0;
    let previous = "";
    let disposed = false;
    type Region = { x: number; y: number; width: number; height: number };
    let pending: Region[] | null = null;
    let inFlight = false;
    const flush = async () => {
      if (disposed || inFlight || pending === null) return;
      const regions = pending;
      pending = null;
      inFlight = true;
      const revision = pointerRevision.current;
      try {
        const status = await invoke<{ pointer: NativePointerEvent }>(
          "hud_report_regions",
          { regions },
        );
        // The RPC covers a listener that was not installed yet. A newer event always
        // wins over its older request's readback, and only one region report is in flight.
        if (
          !disposed &&
          revision === pointerRevision.current &&
          status.pointer.screenId === screenId
        ) {
          setNativePointer(offGuide(status.pointer));
        }
      } catch (error) {
        previous = "";
        if (!disposed) console.error("HUD hit region update failed", error);
      } finally {
        inFlight = false;
        if (!disposed && pending !== null) void flush();
      }
    };
    const report = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        if (disposed) return;
        const regions = [
          ...document.querySelectorAll(
            ".hud-resting-strip, .hud-expanded-surface, .hud-grab-surface, .hud-hover-floor, .hud-dock-end, .hud-guide",
          ),
        ]
          .map((element) => {
            const rect = element.getBoundingClientRect();
            return {
              x: rect.x,
              y: rect.y,
              width: rect.width,
              height: rect.height,
            };
          })
          .filter((rect) => rect.width > 0 && rect.height > 0);
        const serialized = JSON.stringify(regions);
        if (serialized !== previous) {
          previous = serialized;
          pending = regions;
          void flush();
        }
      });
    };
    const stopObserving = observeHUDContours(report);
    window.addEventListener("resize", report);
    report();
    return () => {
      disposed = true;
      stopObserving();
      cancelAnimationFrame(frame);
      window.removeEventListener("resize", report);
      // Native hide/reconciliation owns input teardown. A late empty report from a
      // disposed React effect could otherwise overwrite a replacement effect's regions.
    };
  }, [screenId]);
  return nativePointer;
}
