import { Button } from "@agent-hud/ui/components/button";
import { Card } from "@agent-hud/ui/components/card";
import type { Session, Snapshot } from "@agent-hud/protocol";
import type { useSessionDetail } from "../lib/use-sessions";
import { InlineDetail } from "./SessionDetailView";
import { DetailPlaceholder } from "./SessionPlaceholders";
import { card, type Theme } from "./SessionTheme";
import { useT } from "@agent-hud/ui/format-context";

/** What `useSessionDetail` reads of one session. */
export type DetailState = ReturnType<typeof useSessionDetail>;

/**
 * One session's page, in the session's column or a pick's: the page once the
 * session is known, else its shape in grey (or what failed), and the way to
 * try again.
 */
export function SessionDetailBody({
  session,
  state,
  snapshot,
  theme,
  open,
  selectedEntry,
  onEntry,
  onSession,
  pickedSession = null,
  turnBeside = false,
}: {
  session: Session | undefined;
  state: DetailState;
  snapshot: Snapshot;
  theme: Theme;
  /** Returns to the session, or reveals its log. */
  open: (id: string) => Promise<unknown>;
  selectedEntry: string | null;
  onEntry: (id: string | null) => void;
  onSession: (id: string) => void;
  pickedSession?: string | null;
  turnBeside?: boolean;
}) {
  const t = useT();
  return (
    <>
      {session ? (
        <InlineDetail
          session={session}
          detail={state.detail}
          loading={state.loading}
          error={state.error}
          snapshot={snapshot}
          theme={theme}
          selectedEntry={selectedEntry}
          onEntry={onEntry}
          onSession={onSession}
          pickedSession={pickedSession}
          turnBeside={turnBeside}
          loadMessages={() => void state.loadMessages()}
          loadingMessages={state.loadingMessages}
          reveal={() => {
            void open(session.id).catch((failure) =>
              state.setError(String(failure)),
            );
          }}
        />
      ) : state.error ? (
        <Card style={card(theme)}>
          <span style={{ color: theme.secondary, fontSize: 12 }}>
            {state.error}
          </span>
        </Card>
      ) : (
        <DetailPlaceholder theme={theme} />
      )}
      {state.error && (
        <Button variant="ghost" size="sm" onClick={state.retry}>
          {t("Retry")}
        </Button>
      )}
    </>
  );
}
