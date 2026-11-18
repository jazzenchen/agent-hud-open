// Which of Agent HUD's features a client uses: one it can do (`ClientRow.
// features`, from the runtime), turned on for every agent, and not turned
// off for this one.
import type {
  ClientRow,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";

export type Feature = "live" | "answer" | "talk" | "tools";
export const features: Feature[] = ["live", "answer", "talk", "tools"];

/** A feature's short name, as a client's row lists it. */
export function featureName(feature: Feature, t: Translate) {
  return {
    live: t("Live status"),
    answer: t("Answers"),
    talk: t("Messages"),
    tools: t("HUD tools"),
  }[feature];
}

/** The setting that turns a client's messages off by itself, where it has
 * one (Agent HUD's plugin in Claude Code and DeepSeek Harness). */
export function messagesKey(client: ClientRow) {
  return client.agent === "claude"
    ? "claudeMessages"
    : client.agent === "deepseek"
      ? "deepseekMessages"
      : null;
}

/** Whether `client` can do `feature` at all. */
export function can(client: ClientRow, feature: Feature) {
  return (client.features ?? []).includes(feature);
}

/** Whether `client` uses `feature` now. */
export function uses(
  client: ClientRow,
  feature: Feature,
  snapshot: Snapshot,
  preferences: Record<string, SourcePreferences>,
) {
  const settings = snapshot.settings;
  if (!client.enabled || !can(client, feature)) return false;
  switch (feature) {
    case "live":
      return (
        settings.liveStatus !== false && preferences[client.id]?.live !== false
      );
    case "answer":
      return (
        settings.answerInHud !== false &&
        !(settings.answerOff ?? []).includes(client.id)
      );
    case "talk": {
      const own = messagesKey(client);
      return (
        settings.agentMessages === true && (!own || settings[own] !== false)
      );
    }
    case "tools":
      return settings.hudTools === true;
  }
}
