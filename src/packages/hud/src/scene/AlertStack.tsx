import type {
  Approval,
  IslandEvent,
  RequestDecision,
  Snapshot,
} from "@agent-hud/protocol";
import { snapshotLanguage } from "@agent-hud/ui/format";
import type { useIslandNews } from "../IslandNews";
import { NewsContent } from "../alerts/NewsContent";
import { PendingEventRows } from "../alerts/PendingEventRows";
import { ApprovalContent } from "../permissions/ApprovalContent";

/** The panel's body while an alert is on show: the request or the event, and
 * the ones waiting behind it. */
export function AlertStack({
  requests,
  request,
  event,
  news,
  decide,
  openSettings,
  onStats,
  handoff,
  light,
  snapshot,
  botLogos,
}: {
  requests: Approval[];
  request?: Approval;
  event?: IslandEvent;
  news: ReturnType<typeof useIslandNews>;
  decide?: (id: string, decision: RequestDecision) => void | Promise<void>;
  openSettings?: (page: string) => void;
  onStats?: () => void;
  handoff: (callback?: () => void) => void;
  light: boolean;
  snapshot: Snapshot;
  botLogos: boolean;
}) {
  return (
    <>
      {request ? (
        <ApprovalContent
          requests={requests}
          selected={request}
          select={news.select}
          decide={decide}
          openSettings={openSettings}
          light={light}
          language={snapshotLanguage(snapshot)}
        />
      ) : event ? (
        <NewsContent
          event={event}
          snapshot={snapshot}
          dismiss={news.dismiss}
          onStats={onStats}
          handoff={handoff}
          light={light}
          botLogos={botLogos}
        />
      ) : null}
      <PendingEventRows
        queue={news.queue}
        currentID={(request ?? event)!.id}
        requests={requests}
        select={news.select}
        includeRequests={!request}
        light={light}
        zh={snapshotLanguage(snapshot) === "zh"}
      />
    </>
  );
}
