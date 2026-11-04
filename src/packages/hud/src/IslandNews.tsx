import { useEffect, useRef, useState } from "react";
import type { Approval, IslandEvent, Snapshot } from "@agent-hud/protocol";
import { usePanelServices } from "./PanelContext";
import { agentVendorName, sourceID, vendorKey } from "./model";
import { CompletionAlertView } from "./alerts/CompletionAlertView";
import {
  QuotaAlertView,
  ResetCreditAlertView,
  SignInAlertView,
} from "./alerts/QuotaAlertViews";
import { snapshotLanguage } from "@agent-hud/ui/format";

export { ReminderMark } from "./alerts/ReminderMark";
export {
  CompactAlertView,
  compactAlertDockWidth,
  compactAlertLabels,
  compactQuotaStatus,
} from "./alerts/CompactAlertView";
export {
  CompletionClosedRow,
  PendingEventRows,
  PermissionClosedRow,
  PermissionRowHead,
  WaitedLabel,
  WaitingHeader,
  permissionToolBadge,
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
) {
  const services = usePanelServices(),
    seen = useRef<Set<string> | null>(null),
    seenRequests = useRef<Set<string> | null>(null);
  const [queue, setQueue] = useState<IslandCard[]>([]);
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
    const freshRequests =
      seenRequests.current === null
        ? requests
        : requests.filter((request) => !seenRequests.current!.has(request.id));
    if (seenRequests.current === null) seenRequests.current = new Set();
    const activeRequestIds = new Set(requests.map((request) => request.id));
    for (const id of [...seenRequests.current]) {
      if (!activeRequestIds.has(id)) seenRequests.current.delete(id);
    }
    for (const request of requests) seenRequests.current.add(request.id);
    const visible =
      silenced || !underPointer
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
    setQueue((previous) =>
      reconcileIslandCards(
        silenced
          ? previous.filter((card) => card.type === "request")
          : previous,
        requests,
        visible,
        underPointer ? freshRequests : [],
        inUsagePanel,
      ),
    );
  }, [snapshot.islandEvents?.revision, requestIDs, silenced, underPointer]);
  const current = queue[0];
  const dismiss = () => setQueue(dismissIslandCard);
  useEffect(() => {
    if (!current || held || current.type === "request") return;
    const timer = setTimeout(dismiss, 4000);
    return () => clearTimeout(timer);
  }, [current?.id, held]);
  const select = (id: string) =>
    setQueue((previous) => {
      const at = previous.findIndex((card) => card.id === id);
      if (at <= 0) return previous;
      const next = [...previous];
      [next[0], next[at]] = [next[at], next[0]];
      return next;
    });
  return {
    event: current?.type === "news" ? current.event : undefined,
    inUsagePanel:
      current?.type === "news" ? Boolean(current.inUsagePanel) : false,
    request:
      current?.type === "request"
        ? requests.find((request) => request.id === current.id)
        : undefined,
    queue,
    select,
    dismiss,
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

export function projectName(cwd?: string | null): string | null {
  if (!cwd) return null;
  const trimmed = cwd.replace(/[\\/]+$/, "");
  const name = trimmed.split(/[\\/]/).pop();
  return name && name.length > 0 ? name : null;
}

export function shortenHomePath(path?: string | null): string | null {
  if (!path) return null;
  return path.replace(/^\/Users\/[^/]+/, "~").replace(/^\/home\/[^/]+/, "~");
}

export function NewsContent({
  event,
  snapshot,
  dismiss,
  onStats,
  handoff,
  light = false,
  botLogos = true,
}: {
  event: IslandEvent;
  snapshot: Snapshot;
  dismiss: () => void;
  onStats?: () => void;
  handoff?: (callback?: () => void) => void;
  light?: boolean;
  botLogos?: boolean;
}) {
  const zh = snapshotLanguage(snapshot) === "zh";
  if (event.kind === "completion") {
    return (
      <CompletionAlertView
        event={event}
        snapshot={snapshot}
        dismiss={dismiss}
        onStats={onStats}
        handoff={handoff}
        light={light}
        botLogos={botLogos}
      />
    );
  }
  if (event.kind === "sign-in-needed") {
    return (
      <SignInAlertView
        event={event}
        dismiss={dismiss}
        light={light}
        botLogos={botLogos}
        zh={zh}
      />
    );
  }
  if (event.kind === "reset-credits") {
    return (
      <ResetCreditAlertView
        event={event}
        snapshot={snapshot}
        dismiss={dismiss}
        onStats={onStats}
        light={light}
        botLogos={botLogos}
        zh={zh}
      />
    );
  }
  return (
    <QuotaAlertView
      event={event}
      dismiss={dismiss}
      onStats={onStats}
      light={light}
      botLogos={botLogos}
      zh={zh}
    />
  );
}
