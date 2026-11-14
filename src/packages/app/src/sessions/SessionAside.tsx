import { useState } from "react";
import { Card } from "@agent-hud/ui/components/card";
import type {
  NativeMethod,
  Session,
  SessionDetail,
  Snapshot,
} from "@agent-hud/protocol";
import { useSessionDetail } from "../lib/use-sessions";
import { SessionBackBar } from "./SessionBackBar";
import { SessionDetailBody } from "./SessionDetailBody";
import { CallsPlaceholder } from "./SessionPlaceholders";
import { TurnPanel } from "./TurnPanel";
import type { Pick, SessionLayout } from "./session-layout";
import { card, type Theme } from "./SessionTheme";
import { Bar } from "../statistics/dashboard/Placeholder";
import { useT } from "@agent-hud/ui/format-context";

type Action = (
  method: NativeMethod,
  params?: Record<string, unknown>,
) => Promise<unknown>;

/**
 * The pick's column: a turn of the session beside it, or one of its
 * sub-agents' pages, under a bar that names it and closes it. In one column
 * the pick takes the session's place, so the bar leads back to the session.
 */
export function SessionAside({
  pick,
  session,
  detail,
  layout,
  snapshot,
  theme,
  active,
  action,
  onClose,
  onShift,
}: {
  pick: Pick;
  /** The session the pick was made in. */
  session: Session | undefined;
  detail: SessionDetail | null;
  layout: SessionLayout;
  snapshot: Snapshot;
  theme: Theme;
  active: boolean;
  action: Action;
  onClose: () => void;
  /** A sub-agent of the picked one: the picked one moves to the session's
   * column and its sub-agent opens beside it. */
  onShift: (id: string) => void;
}) {
  const t = useT();
  const one = layout === "one";
  const localWorkers =
    (detail?.session.desktopProject?.localWorkerCount ?? 0) > 0;
  const name =
    pick.kind === "turn"
      ? t("Turn")
      : localWorkers
        ? t("Local execution session")
        : t("Sub-agent");
  return (
    <div style={{ animation: "session-fade-in 200ms ease" }}>
      <SessionBackBar
        label={one ? session?.title || t("Untitled session") : name}
        top={one ? 44 : 0}
        onBack={one ? onClose : undefined}
        onClose={one ? undefined : onClose}
        theme={theme}
      />
      {pick.kind === "turn" ? (
        <PickedTurn id={pick.id} detail={detail} theme={theme} />
      ) : (
        <PickedSession
          key={pick.id}
          id={pick.id}
          parent={detail}
          snapshot={snapshot}
          theme={theme}
          active={active}
          action={action}
          onShift={onShift}
        />
      )}
    </div>
  );
}

/** A turn's calls and facts, as it opened under the chart before. */
function PickedTurn({
  id,
  detail,
  theme,
}: {
  id: string;
  detail: SessionDetail | null;
  theme: Theme;
}) {
  const entry = detail?.turns.find((turn) => turn.id === id);
  return detail && entry ? (
    <TurnPanel
      key={entry.id}
      entry={entry}
      detail={detail}
      byPrompt={detail.turnsBy === "prompt"}
      theme={theme}
    />
  ) : (
    <Card style={{ ...card(theme), gap: 12 }}>
      <Bar width={160} height={13} theme={theme} />
      <Bar width="70%" height={10} theme={theme} />
      <CallsPlaceholder theme={theme} />
    </Card>
  );
}

/** A sub-agent's own page, read on its own; its turns open under its chart. */
function PickedSession({
  id,
  parent,
  snapshot,
  theme,
  active,
  action,
  onShift,
}: {
  id: string;
  parent: SessionDetail | null;
  snapshot: Snapshot;
  theme: Theme;
  active: boolean;
  action: Action;
  onShift: (id: string) => void;
}) {
  const [entry, setEntry] = useState<string | null>(null);
  const observed =
    snapshot.sessions.find((session) => session.id === id) ??
    parent?.children?.find((session) => session.id === id);
  const state = useSessionDetail(id, observed, active, action, entry);
  return (
    <SessionDetailBody
      session={observed ?? state.detail?.session}
      state={state}
      snapshot={snapshot}
      theme={theme}
      open={(session) => action("open_session", { id: session })}
      selectedEntry={entry}
      onEntry={setEntry}
      onSession={onShift}
    />
  );
}
