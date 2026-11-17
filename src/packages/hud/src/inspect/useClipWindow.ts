import { useEffect, useMemo, useRef, useState } from "react";
import type {
  InspectTarget,
  InspectTargets,
  InspectView,
} from "@agent-hud/protocol";
import type { InspectServices } from "../PanelContext";

/** How a send went, for the line under the window's buttons. */
export interface ClipOutcome {
  sent: boolean;
  reason?: string | null;
}

/** Whether `target` is still among the places `list` offers. */
function offered(target: InspectTarget | null, list: InspectTargets) {
  if (!target) return false;
  return target.kind === "session"
    ? list.sessions.some(
        (row) => row.client === target.client && row.id === target.sessionId,
      )
    : list.starts.some((start) => start.client === target.client);
}

/** The newest session that takes words now, for a first send. */
function newest(list: InspectTargets): InspectTarget | null {
  const row = list.sessions.find((session) => session.reachable !== false);
  return row
    ? { kind: "session", client: row.client, sessionId: row.id }
    : null;
}

/** The clipboard window's own state, kept where both the panel and its
 * measuring copy read it, so they draw at the same height: where it sends,
 * the list of places when open, and how the last send went. Opening the
 * window reads the latest copy and asks where words can go. */
export function useClipWindow(
  services: InspectServices | undefined,
  viewing: boolean,
  view: InspectView,
  setView: (view: InspectView) => void,
  [sending, setSending]: [boolean, (sending: boolean) => void],
) {
  const [targets, setTargets] = useState<InspectTargets | null>(null);
  const [target, setTarget] = useState<InspectTarget | null>(null);
  const [picking, setPicking] = useState(false);
  /** The client whose places the open list shows. */
  const [tab, setTab] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<ClipOutcome | null>(null);
  // Each change waits for the one before it, so the host stores them in the
  // order they were typed, and a send goes after the last of them.
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const keep = (work: () => Promise<InspectView>) => {
    const next = queue.current.then(work).then(setView, () => {});
    queue.current = next;
    return next;
  };
  useEffect(() => {
    if (!viewing || !services) {
      setPicking(false);
      setOutcome(null);
      return;
    }
    let live = true;
    services.targets().then(
      (list) => {
        if (!live) return;
        setTargets(list);
        setTarget((current) =>
          offered(current, list) ? current : (list.chosen ?? newest(list)),
        );
      },
      () => {},
    );
    return () => {
      live = false;
    };
  }, [viewing, services]);
  // A copy is read when the window shows it, and so is each new one.
  const unread = viewing && view.clip && !view.clip.content;
  useEffect(() => {
    if (unread && services) void keep(() => services.look());
  }, [unread, view.clip?.id, services]);

  /** Sends the draft, and once it is on its way calls `sent`, before the
   * window lets go of the emptied draft. */
  const send = async (sent?: () => void) => {
    if (!services || !target || sending) return false;
    setSending(true);
    setOutcome(null);
    try {
      await queue.current;
      const result = await services.send(target);
      const went = result.state !== "refused" && result.state !== "expired";
      setOutcome({ sent: went, reason: went ? null : result.reason });
      if (went) sent?.();
      return went;
    } catch (error) {
      setOutcome({ sent: false, reason: String(error) });
      return false;
    } finally {
      setSending(false);
    }
  };
  const actions = services && {
    add: () => keep(() => services.add()),
    edit: (id: string | null, text: string) =>
      keep(() => services.edit(id, text)),
    remove: (id: string) => keep(() => services.remove(id)),
    clear: () => keep(() => services.clear()),
  };
  // What the measuring copy draws again for.
  const shape = useMemo(
    () => ({ view, targets, target, picking, tab, outcome }),
    [view, targets, target, picking, tab, outcome],
  );
  return {
    targets,
    target,
    choose: (next: InspectTarget) => {
      setTarget(next);
      setPicking(false);
    },
    picking,
    /** Opens the list on the chosen place's client, or closes it. */
    togglePicking: () => {
      if (!picking) setTab(target?.client ?? null);
      setPicking(!picking);
    },
    tab,
    setTab,
    sending,
    outcome,
    send,
    actions,
    shape,
  };
}

export type ClipPanel = ReturnType<typeof useClipWindow>;
