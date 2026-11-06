import { useEffect, useRef, useState } from "react";
import type { Approval, IslandEvent, Snapshot } from "@agent-hud/protocol";
import { usePanelServices } from "./PanelContext";
import { agentVendorName, sourceID, vendorKey } from "./model";
import type { EndSide } from "./ends/end-geometry";
import { PARK_AFTER_MS, type ParkedCard } from "./ends/end-queue";
import { CLAIM_GRACE_MS } from "./ends/ends-link";
import { useParkedEnds } from "./ends/useParkedEnds";

export {
  PermissionClosedRow,
  PermissionRowHead,
  WaitingHeader,
} from "./alerts/PendingEventRows";

/** Presentation only: Core is the sole producer/deduplicator of event facts. */
export type IslandCard =
  | { type: "request"; id: string; at: string }
  | {
      type: "news";
      id: string;
      at: string;
      event: IslandEvent;
      inUsagePanel?: boolean;
    };

function promoteOldestWaiting(pending: IslandCard[]): IslandCard[] {
  if (pending.length < 2) return pending;
  // Matches IslandAlertQueue.dismiss(): permission requests have waitingSince = request.at
  // and come before non-persistent events (.distantFuture), which keep arrival order.
  const best = pending.reduce((at, card, index) => {
    const waiting = card.type === "request";
    const prior = pending[at],
      priorWaiting = prior.type === "request";
    return waiting &&
      (!priorWaiting || Date.parse(card.at) < Date.parse(prior.at))
      ? index
      : at;
  }, 0);
  return [pending[best], ...pending.filter((_, index) => index !== best)];
}

export function reconcileIslandCards(
  previous: IslandCard[],
  requests: Approval[],
  visible: IslandEvent[],
  newRequests: Approval[] = requests,
  inUsagePanel = false,
): IslandCard[] {
  const frontRemoved =
    previous.length > 0 &&
    previous[0].type === "request" &&
    !requests.some((request) => request.id === previous[0].id);
  const kept = previous.filter(
    (card) =>
      card.type !== "request" ||
      requests.some((request) => request.id === card.id),
  );
  const next = frontRemoved ? promoteOldestWaiting(kept) : kept;
  for (const request of newRequests) {
    if (!next.some((card) => card.id === request.id)) {
      next.push({
        type: "request",
        id: request.id,
        at: request.createdAt,
      });
    }
  }
  for (const event of visible) {
    if (next[0]?.type === "request" && event.kind !== "completion") continue;
    next.push({
      type: "news",
      id: event.id,
      at: event.at,
      event,
      inUsagePanel: event.kind !== "completion" && inUsagePanel,
    });
  }
  // Persistent requests are never evicted by a burst of informational events.
  let news = 0;
  return next.filter((card) => card.type === "request" || ++news <= 32);
}

export function dismissIslandCard(previous: IslandCard[]): IslandCard[] {
  return promoteOldestWaiting(previous.slice(1));
}

export function useIslandNews(
  snapshot: Snapshot,
  held: boolean,
  requests: Approval[],
  shown: string[],
  silenced = false,
  underPointer = true,
  inUsagePanel = false,
  ends?: {
    /** The end whose panel is open. */
    hovered: EndSide | null;
    /** A card leaves its place beside a mark for its end. */
    onPark?: (card: ParkedCard, source: string) => void;
    /** How often the tail lets its oldest message go; 0, never. */
    fadeMs?: number;
  },
) {
  const services = usePanelServices(),
    seen = useRef<Set<string> | null>(null),
    seenRequests = useRef<Set<string> | null>(null);
  const [queue, setQueue] = useState<IslandCard[]>([]);
  const parked = useParkedEnds({
    requests,
    hovered: ends?.hovered ?? null,
    link: services?.ends,
    fadeMs: ends?.fadeMs,
  });
  const latestRequests = useRef(requests);
  latestRequests.current = requests;
  const items = snapshot.islandEvents?.items ?? [];
  const requestIDs = requests.map((request) => request.id).join("\0");
  useEffect(() => {
    const fresh =
      seen.current === null
        ? []
        : items.filter((event) => !seen.current!.has(event.id));
    if (seen.current === null) seen.current = new Set();
    for (const event of items) seen.current.add(event.id);
    if (seen.current.size > 128)
      seen.current = new Set(items.map((event) => event.id));
    // Requests that wait from before the HUD started go straight to the head.
    const startup = seenRequests.current === null;
    const freshRequests = startup
      ? requests
      : requests.filter((request) => !seenRequests.current!.has(request.id));
    if (startup)
      for (const request of freshRequests)
        parked.park({ type: "request", id: request.id });
    if (seenRequests.current === null) seenRequests.current = new Set();
    const activeRequestIds = new Set(requests.map((request) => request.id));
    for (const id of [...seenRequests.current]) {
      if (!activeRequestIds.has(id)) seenRequests.current.delete(id);
    }
    for (const request of requests) seenRequests.current.add(request.id);
    const eligible = silenced
      ? []
      : fresh.filter(
          (event) =>
            event.islandAlert !== false &&
            shown.includes(vendorKey(event.source)) &&
            (event.kind !== "completion" ||
              (services?.sources?.[sourceID(snapshot, event.source)]?.live !==
                false &&
                services?.sources?.[sourceID(snapshot, event.source)]
                  ?.reminders !== false)),
        );
    const coming = startup ? [] : freshRequests;
    const visible = underPointer ? eligible : [];
    // The pointer's screen shows the new cards and parks them, on every
    // screen. Should no screen with a HUD have the pointer, each parks them
    // itself once none has claimed them.
    if (underPointer) parked.claim([...visible, ...coming].map((c) => c.id));
    else if (eligible.length || coming.length) {
      const cards: ParkedCard[] = [
        ...coming.map((r) => ({ type: "request" as const, id: r.id })),
        ...eligible.map((e) => ({
          type: "news" as const,
          id: e.id,
          at: e.at,
          event: e,
        })),
      ];
      setTimeout(() => {
        for (const card of cards)
          if (!parked.claimed(card.id)) parked.park(card);
      }, CLAIM_GRACE_MS);
    }
    setQueue((previous) =>
      reconcileIslandCards(
        silenced
          ? previous.filter((card) => card.type === "request")
          : previous,
        requests,
        visible,
        underPointer ? coming : [],
        inUsagePanel,
      ),
    );
  }, [snapshot.islandEvents?.revision, requestIDs, silenced, underPointer]);
  const current = queue[0];
  const latestCurrent = useRef(current);
  latestCurrent.current = current;
  const dismiss = () => setQueue(dismissIslandCard);
  /** The card `id` leaves its mark for its end now, if it is still on show. */
  const park = (id: string) => {
    const card = latestCurrent.current;
    if (card?.id !== id) return;
    const parkedCard: ParkedCard =
      card.type === "request"
        ? { type: "request", id: card.id }
        : { ...card, type: "news" };
    const source =
      card.type === "news"
        ? card.event.source
        : latestRequests.current.find((r) => r.id === card.id)?.source;
    ends?.onPark?.(parkedCard, source ?? "");
    parked.park(parkedCard);
    dismiss();
  };
  // A card has its moment beside its agent's mark, then parks at its end.
  useEffect(() => {
    if (!current || held) return;
    const timer = setTimeout(
      () => park(current.id),
      PARK_AFTER_MS[current.type],
    );
    return () => clearTimeout(timer);
  }, [current?.id, held]);
  const select = (id: string) => {
    if (parked.view) return parked.pick(id);
    setQueue((previous) => {
      const at = previous.findIndex((card) => card.id === id);
      if (at <= 0) return previous;
      const next = [...previous];
      [next[0], next[at]] = [next[at], next[0]];
      return next;
    });
  };
  // The card on show beside a mark, whatever an end shows.
  const fresh = {
    event: current?.type === "news" ? current.event : undefined,
    request:
      current?.type === "request"
        ? requests.find((request) => request.id === current.id)
        : undefined,
  };
  const message = parked.message;
  return {
    event: parked.view ? message?.event : fresh.event,
    inUsagePanel:
      !parked.view && current?.type === "news"
        ? Boolean(current.inUsagePanel)
        : false,
    request: parked.view ? parked.request : fresh.request,
    fresh,
    queue,
    select,
    park,
    dismiss:
      parked.view === "tail"
        ? () => message && parked.drop(message.id)
        : dismiss,
    ends: parked,
  };
}

export const newsTitle = (event: IslandEvent, zh = false) =>
  event.kind === "completion"
    ? zh
      ? "有新回复"
      : "New reply"
    : event.kind === "quota-reset"
      ? zh
        ? "额度已重置"
        : "Quota reset"
      : event.kind === "reset-credits"
        ? zh
          ? "获得使用重置"
          : "Usage resets added"
        : event.kind === "sign-in-needed"
          ? zh
            ? "需要重新登录"
            : "Sign in again"
          : event.kind === "hooks-review-needed"
            ? zh
              ? "回调待允许"
              : "Allow its hooks"
            : zh
              ? "额度预警"
              : "Running low";

export const compactDuration = (seconds: number) => {
  const total = Math.max(0, Math.round(seconds)),
    h = Math.floor(total / 3600),
    m = Math.floor((total % 3600) / 60);
  return h > 0 ? `${h}h${String(m).padStart(2, "0")}m` : `${m}m`;
};

export function clientDisplayName(source: string): string {
  const normalized = source.trim().toLowerCase();
  if (normalized === "codex" || normalized === "codex-cli") return "Codex";
  return agentVendorName(source);
}
