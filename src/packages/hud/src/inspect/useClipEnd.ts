import { useEffect, useRef, useState } from "react";
import type { InspectView } from "@agent-hud/protocol";
import type { EndSide } from "../ends/end-geometry";
import { useEndFade } from "../ends/useEndFade";
import type { HUDConfig } from "../model";
import { usePanelServices } from "../PanelContext";
import { useClipWindow } from "./useClipWindow";

const EMPTY: InspectView = { clip: null, items: [], note: "" };

/** How long the window, called up, stays up unvisited. */
const PRESENT_MS = 20_000;

/** The host's state of the clipboard window, as every HUD window hears it,
 * and whether the window is held up: when it is called up (a pick from the
 * browser, ⌥Z, or the markup window closing), until the pointer has been on
 * the window and left it, or for `PRESENT_MS`. The markup window itself
 * holds nothing: the HUD goes on as usual while it is open. */
export function useInspectView(
  pointerInside: boolean,
  setHovered: (end: EndSide | null) => void,
) {
  const services = usePanelServices()?.inspect;
  const [view, setView] = useState<InspectView>(EMPTY);
  useEffect(() => {
    if (!services) return;
    let live = true;
    services.state().then(
      (next) => live && setView(next),
      () => {},
    );
    const stop = services.subscribe(setView);
    return () => {
      live = false;
      stop();
    };
  }, [services]);
  const [presenting, setPresenting] = useState(false);
  const visited = useRef(false);
  const shown = useRef(view.presented ?? null);
  useEffect(() => {
    const pick = view.presented ?? null;
    if (!pick || pick === shown.current) return;
    shown.current = pick;
    visited.current = false;
    setHovered("clip");
    setPresenting(true);
    const timer = setTimeout(() => setPresenting(false), PRESENT_MS);
    return () => clearTimeout(timer);
  }, [view.presented]);
  useEffect(() => {
    if (!presenting) return;
    if (pointerInside) visited.current = true;
    else if (visited.current) setPresenting(false);
  }, [pointerInside, presenting]);
  return {
    view,
    setView,
    hold: presenting,
  };
}

/** The clipboard's end of the dock: the host's state of the clipboard
 * window, whether its bubble shows, its countdown, and the window's own
 * state while it shows.
 * A copy's bubble shows for `clipboardFadeSeconds`, counted again from the
 * start once the pointer has been on it; with something in the draft, or a
 * picture open in the markup window, it stays and its clock stops. */
export function useClipEnd(
  config: HUDConfig,
  hovered: EndSide | null,
  open: boolean,
  {
    view,
    setView,
  }: Pick<ReturnType<typeof useInspectView>, "view" | "setView">,
) {
  const services = usePanelServices()?.inspect;
  const [faded, setFaded] = useState<string | null>(null);
  // Called up again, the latest copy shows again too.
  useEffect(() => setFaded(null), [view.presented]);
  const copy = view.clip?.id ?? null;
  // A picture open in the markup window holds the bubble too.
  const holding =
    view.items.length > 0 || view.note.trim() !== "" || Boolean(view.marking);
  // A send in progress keeps the window until it hands the HUD back.
  const [sending, setSending] = useState(false);
  // A pick from the browser shows with the bubble off too: only copies
  // follow the switch.
  const shown =
    Boolean(services) &&
    (holding ||
      sending ||
      (config.clipboardBubble && copy !== null && faded !== copy));
  const viewing = open && hovered === "clip" && shown;
  const fade = useEndFade({
    active: !holding && copy !== null,
    begins: copy,
    paused: viewing,
    ms: config.clipboardFadeSeconds * 1000,
    done: () => setFaded(copy),
  });
  const panel = useClipWindow(services, viewing, view, setView, [
    sending,
    setSending,
  ]);
  return {
    view,
    panel,
    shown,
    viewing,
    /** How many wait in the draft, for the end's badge. */
    count: view.items.length,
    fade,
  };
}

export type ClipEnd = ReturnType<typeof useClipEnd>;
