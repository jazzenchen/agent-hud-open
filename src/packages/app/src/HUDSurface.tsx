import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { HUDScene } from "@agent-hud/hud/HUDScene";
import type {
  HUDDragStartArgs,
  HUDFlight,
  HUDFlightCommand,
  HUDNativePointer,
} from "@agent-hud/hud/HUDScene";
import type {
  HUDConfig,
  HUDDecision,
  HUDUpdateNotice,
} from "@agent-hud/hud/model";
import type { Direction } from "@agent-hud/hud/bots/mark-director";
import type { Snapshot, NativeMethod } from "@agent-hud/protocol";
import type {
  NativeHUDGeometry,
  NativeHUDMonitor,
} from "./lib/use-hud-geometry";

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
/** Only outer HUD geometry wakes native reports; animated bot descendants never do. */
function observeHUDContours(report: () => void) {
  const selector =
    ".hud-scene, .hud-resting-strip, .hud-expanded-surface, .hud-grab-surface, .hud-hover-floor";
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

export default function NativeHUDSurface({
  screenId,
  snapshot,
  config,
  geometry,
  monitors,
  botDirections,
  glowHidden,
  action,
}: {
  screenId: string;
  snapshot: Snapshot;
  config: HUDConfig;
  geometry: NativeHUDGeometry | null;
  monitors: NativeHUDMonitor[];
  botDirections?: Record<string, Direction>;
  glowHidden?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
}) {
  const [textFocused, setTextFocused] = useState(false);
  const liveRequests = useRef(new Set<string>());
  liveRequests.current = new Set(
    snapshot.approvals.map((request) => request.id),
  );
  const requestIdentity = snapshot.approvals
    .map((request) => request.id)
    .join("\0");
  const textFocusOwner = useRef<{
    verify: () => void;
    leave: () => void;
  } | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    type TextControl = HTMLInputElement | HTMLTextAreaElement;
    const editable = (target: EventTarget | null): TextControl | null => {
      if (!(target instanceof Element)) return null;
      const element = target.closest("textarea, input");
      if (
        !(element instanceof HTMLTextAreaElement) &&
        !(element instanceof HTMLInputElement)
      )
        return null;
      if (
        element instanceof HTMLInputElement &&
        ![
          "text",
          "search",
          "email",
          "url",
          "tel",
          "password",
          "number",
        ].includes(element.type)
      )
        return null;
      if (
        element.disabled ||
        element.readOnly ||
        element.closest("[inert], [aria-hidden=true]")
      )
        return null;
      return element;
    };
    let element: TextControl | null = null;
    let wanted = false,
      disposed = false,
      epoch = 0;
    // Words selected outside a text field also take the keyboard, so ⌘C
    // copies them instead of reaching the app in front. Unlike typing, this
    // leaves the panel's opening and closing alone.
    let selecting = false,
      pressed = false;
    const selected = (): Element | null => {
      const selection = document.getSelection();
      if (!selection || selection.isCollapsed || !selection.rangeCount)
        return null;
      const node = selection.getRangeAt(0).commonAncestorContainer;
      const owner = node instanceof Element ? node : node.parentElement;
      if (
        !owner?.isConnected ||
        owner.closest("[inert], [aria-hidden=true]") ||
        editable(owner)
      )
        return null;
      return owner;
    };
    let serial: Promise<void> = Promise.resolve();
    // A release carries this surface's identity, so a late unmount cannot clear
    // another surface's newer lease after React remounts it.
    const leaseId = crypto.randomUUID();
    const release = () => {
      if (!wanted) return;
      wanted = false;
      selecting = false;
      setTextFocused(false);
      changed.disconnect();
      epoch += 1;
      const previous = element;
      element = null;
      if (document.activeElement === previous) previous?.blur();
      serial = serial
        .catch(() => undefined)
        .then(async () => {
          await invoke("hud_text_focus", { active: false, leaseId });
        })
        .catch((error) => {
          if (!disposed)
            console.error("HUD keyboard focus release failed", error);
        });
    };
    const verified = (target: TextControl | null) => {
      if (!target?.isConnected || target.closest("[inert], [aria-hidden=true]"))
        return false;
      const requestId =
        target.closest<HTMLElement>("[data-request-id]")?.dataset.requestId;
      return !requestId || liveRequests.current.has(requestId);
    };
    const owner = {
      verify: () => {
        if (!wanted || (selecting && pressed)) return;
        if (selecting ? !selected() : !verified(element)) release();
      },
      // The pointer left the HUD: its panel closes, so the selection goes
      // and the keys go back.
      leave: () => {
        if (!selecting) return;
        document.getSelection()?.removeAllRanges();
        release();
      },
    };
    textFocusOwner.current = owner;
    const acquire = (target: TextControl, refocus: boolean) => {
      element = target;
      wanted = true;
      selecting = false;
      setTextFocused(true);
      const root = document.getElementById("root");
      if (root) changed.observe(root, { childList: true, subtree: true });
      const captured = ++epoch;
      serial = serial
        .catch(() => undefined)
        .then(async () => {
          if (disposed || !wanted || captured !== epoch || !verified(target))
            return;
          await invoke("hud_text_focus", { active: true, leaseId });
          if (
            refocus &&
            !disposed &&
            wanted &&
            captured === epoch &&
            verified(target)
          )
            target.focus({ preventScroll: true });
        })
        .catch((error) => {
          if (!disposed && captured === epoch) {
            console.error("HUD keyboard focus failed", error);
            release();
          }
        });
    };
    const holdSelection = () => {
      if (wanted || !selected()) return;
      wanted = true;
      selecting = true;
      const root = document.getElementById("root");
      if (root) changed.observe(root, { childList: true, subtree: true });
      const captured = ++epoch;
      serial = serial
        .catch(() => undefined)
        .then(async () => {
          if (
            disposed ||
            !wanted ||
            !selecting ||
            captured !== epoch ||
            !selected()
          )
            return;
          await invoke("hud_text_focus", { active: true, leaseId });
        })
        .catch((error) => {
          if (!disposed && captured === epoch) {
            console.error("HUD keyboard focus failed", error);
            release();
          }
        });
    };
    const pointerDown = (event: PointerEvent) => {
      if (!event.isTrusted || event.button !== 0) return;
      pressed = true;
      const target = editable(event.target);
      if (!target || !verified(target)) {
        // A press may start a new selection: a selection's keys wait for it
        // to end rather than going back mid-drag.
        if (!selecting) release();
        return;
      }
      acquire(target, true);
    };
    const pointerUp = (event: PointerEvent) => {
      if (!event.isTrusted || event.button !== 0) return;
      pressed = false;
      if (selected()) holdSelection();
      else if (selecting) release();
    };
    const selectionChange = () => {
      if (selecting && !pressed && !selected()) release();
    };
    const focusIn = (event: FocusEvent) => {
      const target = editable(event.target);
      if (!target || !verified(target)) return;
      if (!wanted || element !== target) acquire(target, false);
    };
    const focusOut = () =>
      queueMicrotask(() => {
        if (disposed || !wanted || selecting) return;
        const next = editable(document.activeElement);
        if (next && verified(next)) element = next;
        else release();
      });
    const changed = new MutationObserver(owner.verify);
    document.addEventListener("pointerdown", pointerDown, true);
    document.addEventListener("pointerup", pointerUp, true);
    document.addEventListener("selectionchange", selectionChange);
    document.addEventListener("focusin", focusIn, true);
    document.addEventListener("focusout", focusOut, true);
    window.addEventListener("blur", release);
    window.addEventListener("pagehide", release);
    return () => {
      disposed = true;
      release();
      changed.disconnect();
      document.removeEventListener("pointerdown", pointerDown, true);
      document.removeEventListener("pointerup", pointerUp, true);
      document.removeEventListener("selectionchange", selectionChange);
      document.removeEventListener("focusin", focusIn, true);
      document.removeEventListener("focusout", focusOut, true);
      window.removeEventListener("blur", release);
      window.removeEventListener("pagehide", release);
      if (textFocusOwner.current === owner) textFocusOwner.current = null;
    };
  }, [screenId]);
  useEffect(() => {
    textFocusOwner.current?.verify();
  }, [requestIdentity]);
  const [dragEndSequence, setDragEndSequence] = useState(0);
  const [nativePointer, setNativePointer] = useState<
    HUDNativePointer | undefined
  >(() => (isTauri() ? outsidePointer : undefined));
  const pointerInside = nativePointer?.inside;
  useEffect(() => {
    if (pointerInside === false) textFocusOwner.current?.leave();
  }, [pointerInside]);
  const pointerRevision = useRef(0);
  const lastPreview = useRef<Pick<
    HUDConfig,
    "mode" | "edge" | "offset"
  > | null>(null);
  // After a drop the saved placement reaches this window through Settings a moment later.
  // Keep showing the dropped placement until then, instead of jumping back to the old one.
  const [held, setHeld] = useState<Pick<
    HUDConfig,
    "mode" | "edge" | "offset"
  > | null>(null);
  const [dragPreview, setDragPreview] = useState<Pick<
    HUDConfig,
    "mode" | "edge" | "offset" | "monitorIndex"
  > | null>(null);
  useEffect(() => {
    if (!held) return;
    if (
      config.mode === held.mode &&
      config.edge === held.edge &&
      Math.abs(config.offset - held.offset) < 1e-6
    ) {
      setHeld(null);
      return;
    }
    const timer = setTimeout(() => setHeld(null), 800);
    return () => clearTimeout(timer);
  }, [held, config.mode, config.edge, config.offset]);
  const shownConfig = held ? { ...config, ...held } : config;
  const configure = useCallback(() => {
    void invoke("hud_show_main");
  }, []);
  // Statistics opens on Tokens (Open's footer button), or where a session row points.
  const stats = useCallback((target?: { tab?: string; sessionId?: string }) => {
    void invoke("open_statistics", {
      tab: target?.tab ?? "tokens",
      sessionId: target?.sessionId,
    });
  }, []);
  const looked = useCallback((keys: string[] | null) => {
    void invoke("hud_bot_looked", { keys });
  }, []);
  const beginDrag = useCallback(async (args: HUDDragStartArgs) => {
    await invoke("hud_drag_start", { ...args });
  }, []);
  const decide = useCallback(
    async (id: string, decision: HUDDecision) => {
      await action("answer_request", { id, decision });
    },
    [action],
  );
  // `--demo-script` panel steps; only the primary screen's HUD follows them.
  const [panelCommand, setPanelCommand] = useState<{
    open: boolean;
    sequence: number;
  }>();
  useEffect(() => {
    if (!isTauri()) return;
    const subscription = listen<boolean>("demo:panel", ({ payload }) =>
      setPanelCommand((previous) => ({
        open: payload,
        sequence: (previous?.sequence ?? 0) + 1,
      })),
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
  // The product's newer version (the panel footer's arrow): read when the
  // page starts, as a hidden page misses the event, then kept up to date.
  const [update, setUpdate] = useState<HUDUpdateNotice | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let heard = false;
    const subscription = listen<HUDUpdateNotice | null>(
      "hud:update",
      ({ payload }) => {
        heard = true;
        setUpdate(payload);
      },
    );
    void invoke<HUDUpdateNotice | null>("hud_update_notice")
      .then((notice) => {
        if (!heard) setUpdate(notice);
      })
      .catch(() => {});
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
  const openUpdate = useCallback(() => {
    void invoke("open_update");
  }, []);
  // A product's flights between two agents' marks; every screen's HUD plays them.
  const [flights, setFlights] = useState<readonly HUDFlightCommand[]>([]);
  useEffect(() => {
    if (!isTauri()) return;
    const subscription = listen<HUDFlight>("hud:flight", ({ payload }) =>
      setFlights((previous) => [
        ...previous.slice(-15),
        {
          flight: payload,
          sequence: (previous.at(-1)?.sequence ?? 0) + 1,
          at: performance.now(),
        },
      ]),
    );
    return () => void subscription.then((unlisten) => unlisten());
  }, []);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    setNativePointer(outsidePointer);
    const pointerSubscription = listen<NativePointerEvent>(
      "hud:pointer",
      (event) => {
        if (disposed || event.payload.screenId !== screenId) return;
        pointerRevision.current += 1;
        setNativePointer(event.payload);
      },
    );
    const previewSubscription = listen<
      { screenId: string } & Pick<
        HUDConfig,
        "mode" | "edge" | "offset" | "monitorIndex"
      >
    >("hud:drag-preview", (event) => {
      if (disposed || event.payload.screenId !== screenId) return;
      const { mode, edge, offset, monitorIndex } = event.payload;
      lastPreview.current = { mode, edge, offset };
      setDragPreview({ mode, edge, offset, monitorIndex });
    });
    const subscription = listen<{ screenId: string; cancelled: boolean }>(
      "hud:drag-end",
      (event) => {
        if (!disposed && event.payload.screenId === screenId) {
          if (!event.payload.cancelled && lastPreview.current)
            setHeld(lastPreview.current);
          lastPreview.current = null;
          setDragPreview(null);
          setDragEndSequence((sequence) => sequence + 1);
        }
      },
    );
    return () => {
      disposed = true;
      void previewSubscription.then((unlisten) => unlisten());
      void subscription.then((unlisten) => unlisten());
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
          setNativePointer(status.pointer);
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
            ".hud-resting-strip, .hud-expanded-surface, .hud-grab-surface, .hud-hover-floor",
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
  if (isTauri() && (!geometry || geometry.screenId !== screenId)) return null;
  const primary =
    monitors.find((m) => m.screenId === screenId)?.primary ??
    monitors[0]?.screenId === screenId;
  const underPointer = (() => {
    if (!isTauri() || !geometry || monitors.length <= 1) return true;
    if (!nativePointer?.point) return primary;
    const globalX = nativePointer.point.x + geometry.canvas.x;
    const globalY = nativePointer.point.y + geometry.canvas.y;
    const screen = geometry.screen;
    if (
      globalX >= screen.x &&
      globalX <= screen.x + screen.width &&
      globalY >= screen.y &&
      globalY <= screen.y + screen.height
    ) {
      return true;
    }
    for (const monitor of monitors) {
      if (monitor.screenId === screenId) continue;
      const scale =
        geometry.coordinateSpace === "global-logical-top-left"
          ? 1
          : monitor.scaleFactor || 1;
      const mx = monitor.bounds.x / scale;
      const my = monitor.bounds.y / scale;
      const mw = monitor.bounds.width / scale;
      const mh = monitor.bounds.height / scale;
      if (
        globalX >= mx &&
        globalX <= mx + mw &&
        globalY >= my &&
        globalY <= my + mh
      ) {
        return false;
      }
    }
    return primary;
  })();
  const notchFrame = geometry
    ? (geometry.notchInViewport ?? {
        x:
          geometry.screen.x +
          (geometry.screen.width - 200) / 2 -
          geometry.canvas.x,
        y: geometry.screen.y - geometry.canvas.y,
        width: 200,
        height: geometry.menuBarHeight,
      })
    : undefined;
  return (
    <HUDScene
      snapshot={snapshot}
      config={shownConfig}
      dragPreview={dragPreview}
      dragEndSequence={dragEndSequence}
      nativePointer={nativePointer}
      underPointer={underPointer}
      textFocused={textFocused}
      botDirections={botDirections}
      glowHidden={glowHidden}
      onBotLooked={looked}
      notchFrame={notchFrame}
      height={geometry?.canvas.height ?? window.innerHeight}
      onConfigure={configure}
      onStats={stats}
      update={update}
      onUpdate={openUpdate}
      onDragStart={beginDrag}
      onRequestDecision={decide}
      panelCommand={primary ? panelCommand : undefined}
      flights={flights}
    />
  );
}
