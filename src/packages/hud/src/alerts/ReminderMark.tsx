import type { Approval, IslandEvent } from "@agent-hud/protocol";
import { BotMark } from "../BotMark";
import { botState, cardForReply } from "../bots/bot-state";

export function ReminderMark({
  request,
  event,
  size,
  light = false,
  botLogos = true,
}: {
  request?: Approval;
  event?: IslandEvent;
  size: number;
  light?: boolean;
  botLogos?: boolean;
}) {
  if (request) {
    const look =
      request.kind === "plan"
        ? "plan"
        : request.kind === "question"
          ? "question"
          : "permission";
    return (
      <BotMark
        source={request.source}
        size={size}
        light={light}
        enabled={botLogos}
        staysBot
        awake
        state={botState({ look })}
      />
    );
  }
  if (!event) return null;
  if (event.kind === "completion") {
    const startSec = event.startedAt
      ? Date.parse(event.startedAt) / 1000
      : null;
    const endSec = Date.parse(event.at) / 1000;
    const card = cardForReply(
      startSec !== null && Number.isFinite(startSec) ? startSec : null,
      Number.isFinite(endSec) ? endSec : 0,
    );
    return (
      <BotMark
        source={event.source}
        size={size}
        light={light}
        enabled={botLogos}
        staysBot
        awake
        state={botState({ look: "done", card })}
      />
    );
  }
  if (event.kind === "quota-reset" || event.kind === "reset-credits") {
    return (
      <BotMark
        source={event.source}
        size={size}
        light={light}
        enabled={botLogos}
        staysBot
        awake
        state={botState({ look: "done", card: "reset" })}
      />
    );
  }
  const exhausted = event.crossing === "exhausted";
  return (
    <BotMark
      source={event.source}
      size={size}
      light={light}
      enabled={false}
      staysBot={false}
      awake
      state={botState({ badge: exhausted ? "limited" : null })}
    />
  );
}
