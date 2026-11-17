import { useEffect, useRef, useState } from "react";
import type { InspectView } from "@agent-hud/protocol";
import type { EndSide } from "../ends/end-geometry";
import type { EndFade } from "../ends/useParkedEnds";
import type { HUDConfig } from "../model";
import { usePanelServices } from "../PanelContext";
import { useClipWindow } from "./useClipWindow";

const EMPTY: InspectView = { clip: null, items: [], note: "" };

/** A copy's bubble lasts `ms` from the copy, the clock stopping while
 * `paused` (its window is open). A new copy starts it again; with no copy
 * (`key` null) or an `ms` of 0 it does not run. */
function useClipFade(
  key: string | null,
  paused: boolean,
  ms: number,
  done: () => void,
): EndFade {
  const [cycle, setCycle] = useState(0);
  const left = useRef(ms);
  const finish = useRef(done);
  finish.current = done;
  const active = key !== null && ms > 0;
  useEffect(() => {
    if (!active) return;
    left.current = ms;
    setCycle((n) => n + 1);
  }, [key, active, ms]);
  useEffect(() => {
    if (!active || paused) return;
    const start = performance.now();
    let fired = false;
    const timer = setTimeout(() => {
      fired = true;
      finish.current();
    }, left.current);
    return () => {
      clearTimeout(timer);
      if (!fired)
        left.current = Math.max(0, left.current - (performance.now() - start));
    };
  }, [active, paused, cycle]);
  return { cycle, paused: paused || !active, ms: active ? ms : 0 };
}

/** How long a pick from the browser holds the window up unvisited. */
const PRESENT_MS = 20_000;

/** The host's state of the clipboard window, as every HUD window hears it,
 * and whether the window is held up: while a picture is in the markup
 * window, and for a pick from the browser, until the pointer has been on
 * the window and left it, or for `PRESENT_MS`. */
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
    hold: Boolean(view.marking) || presenting,
  };
}

/** The clipboard's end of the dock: the host's state of the clipboard
 * window, whether its bubble shows, its countdown, and the window's own
 * state while it shows.
 * A copy's bubble shows for `clipboardFadeSeconds`; with something in the
 * draft, or a picture open in the markup window, it stays and its clock
 * stops. */
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
  const copy = view.clip?.id ?? null;
  // A picture open in the markup window holds the bubble too.
  const holding =
    view.items.length > 0 || view.note.trim() !== "" || Boolean(view.marking);
  // A send in progress keeps the window until it hands the HUD back.
  const [sending, setSending] = useState(false);
  const shown =
    Boolean(services) &&
    config.clipboardBubble &&
    (holding || sending || (copy !== null && faded !== copy));
  const viewing = open && hovered === "clip" && shown;
  const fade = useClipFade(
    holding ? null : copy,
    viewing,
    config.clipboardFadeSeconds * 1000,
    () => setFaded(copy),
  );
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
