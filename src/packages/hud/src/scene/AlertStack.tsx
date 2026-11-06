import type {
  Approval,
  IslandEvent,
  RequestDecision,
  Snapshot,
} from "@agent-hud/protocol";
import { snapshotLanguage } from "@agent-hud/ui/format";
import type { useIslandNews } from "../IslandNews";
import { usePanelServices } from "../PanelContext";
import { NewsContent } from "../alerts/NewsContent";
import { PendingEventRows } from "../alerts/PendingEventRows";
import { ApprovalContent } from "../permissions/ApprovalContent";
import { MessageRows, MessagesHeader } from "../ends/MessageRows";

/** The panel's body while an alert is on show: the request or the event, and
 * the ones waiting behind it. An open end shows what waits there: the head
 * its requests, the tail its messages, newest first. */
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
  const zh = snapshotLanguage(snapshot) === "zh";
  const ends = news.ends;
  const services = usePanelServices();
  // Back to the session a request comes from, where Agent HUD can reach it.
  // The request still waits, so its card goes to the head as it would
  // unattended, and the HUD steps aside for the session's window.
  const backToSession = (request: Approval) => {
    const session = snapshot.sessions.find((s) => s.id === request.sessionId);
    const open = services?.openSession;
    if (!session?.navigationTarget || !open) return undefined;
    return async () => {
      if ((await open(session)) === false) return false;
      news.park(request.id);
      handoff();
      return true;
    };
  };
  return (
    <>
      {ends.view === "tail" && (
        <MessagesHeader
          count={ends.tail.length}
          clear={ends.clear}
          light={light}
          zh={zh}
        />
      )}
      {request ? (
        <ApprovalContent
          requests={requests}
          selected={request}
          select={news.select}
          decide={decide}
          openSettings={openSettings}
          backToSession={backToSession}
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
      {ends.view === "tail" ? (
        <MessageRows
          items={ends.tail}
          currentID={(request ?? event)!.id}
          select={news.select}
          drop={ends.drop}
          light={light}
          zh={zh}
        />
      ) : (
        !ends.view && (
          <PendingEventRows
            queue={news.queue}
            currentID={(request ?? event)!.id}
            requests={requests}
            select={news.select}
            includeRequests={!request}
            light={light}
            zh={zh}
          />
        )
      )}
    </>
  );
}
