// Which of Agent HUD's features a client uses: one it can do (`ClientRow.
// features`, from the runtime), turned on for every agent, and not turned
// off for this one.
import type {
  ClientRow,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { knownClients, type MCPClientsStatus } from "../agent-plugins/services";

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

/** The client in the host's list of those HUD tools can be added to
 * (hud_tools::CLIENTS); null for one they can't. Claude Code's command line
 * and desktop app share one setup, as Codex's do. */
export function toolsClient(client: ClientRow) {
  const id =
    ({ claude: "claudeCode", "trae-cn": "traeCn" } as Record<string, string>)[
      client.agent
    ] ?? client.agent;
  return knownClients.find((known) => known.id === id) ?? null;
}

/** Whether `client` can do `feature` at all. */
export function can(client: ClientRow, feature: Feature) {
  return (client.features ?? []).includes(feature);
}

/** Whether `client`'s permission requests are allowed at once, with no
 * card: approving automatically is on, its answers are in the HUD, and it
 * was not left out on its own page. */
export function autoApproves(
  client: ClientRow,
  snapshot: Snapshot,
  preferences: Record<string, SourcePreferences>,
) {
  return (
    snapshot.settings.autoApprove === true &&
    uses(client, "answer", snapshot, preferences) &&
    !(snapshot.settings.autoApproveOff ?? []).includes(client.id)
  );
}

/** Whether `client` uses `feature` now; HUD tools once they are added to
 * it, when `added` has been read. */
export function uses(
  client: ClientRow,
  feature: Feature,
  snapshot: Snapshot,
  preferences: Record<string, SourcePreferences>,
  added?: MCPClientsStatus | null,
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
    case "tools": {
      const id = toolsClient(client)?.id;
      const entry = added?.clients.find((item) => item.id === id);
      return (
        settings.hudTools === true && (!added || Boolean(entry?.installed))
      );
    }
  }
}
