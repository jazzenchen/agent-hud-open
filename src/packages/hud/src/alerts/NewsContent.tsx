import type { IslandEvent, Snapshot } from "@agent-hud/protocol";
import { CompletionAlertView } from "./CompletionAlertView";
import { HooksReviewAlertView } from "./HooksReviewAlertView";
import {
  QuotaAlertView,
  ResetCreditAlertView,
  SignInAlertView,
} from "./QuotaAlertViews";

/** An island event's card, by its kind. */
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
      />
    );
  }
  if (event.kind === "hooks-review-needed") {
    return (
      <HooksReviewAlertView
        event={event}
        dismiss={dismiss}
        light={light}
        botLogos={botLogos}
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
    />
  );
}
