// Which of Agent HUD's features a client uses: one it can do (`ClientRow.
// features`, from the runtime) and turned on for it. A client's own page
// wins over the switch for every agent, which only sets the default
// (agents::switches in the runtime).
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

type Switched = "answer" | "autoApprove";
const switches: Record<
  Switched,
  {
    main: "answerInHud" | "autoApprove";
    fallback: boolean;
    on: "answerOn" | "autoApproveOn";
    off: "answerOff" | "autoApproveOff";
  }
> = {
  answer: {
    main: "answerInHud",
    fallback: true,
    on: "answerOn",
    off: "answerOff",
  },
  autoApprove: {
    main: "autoApprove",
    fallback: false,
    on: "autoApproveOn",
    off: "autoApproveOff",
  },
};

/** Whether a switched feature is on for the client `id`: its own page,
 * otherwise the switch for every agent. */
export function switchedOn(
  settings: Snapshot["settings"],
  feature: Switched,
  id: string,
) {
  const keys = switches[feature];
  if ((settings[keys.off] ?? []).includes(id)) return false;
  if ((settings[keys.on] ?? []).includes(id)) return true;
  return settings[keys.main] ?? keys.fallback;
}

/** The settings that turn a switched feature on or off for the client `id`
 * alone, as [key, value] pairs to save. */
export function switchFor(
  settings: Snapshot["settings"],
  feature: Switched,
  id: string,
  value: boolean,
): [string, string[]][] {
  const keys = switches[feature];
  const on = (settings[keys.on] ?? []).filter((other) => other !== id);
  const off = (settings[keys.off] ?? []).filter((other) => other !== id);
  return [
    [keys.on, value ? [...on, id] : on],
    [keys.off, value ? off : [...off, id]],
  ];
}

/** Whether `client`'s live status is on: its own page, otherwise the switch
 * for every agent (the runtime puts that into `preferences`). */
export function liveOn(
  client: ClientRow,
  snapshot: Snapshot,
  preferences: Record<string, SourcePreferences>,
) {
  return preferences[client.id]?.live ?? snapshot.settings.liveStatus !== false;
}

/** Whether `client`'s permission requests are allowed at once, with no
 * card: its answers are in the HUD and approving automatically is on for
 * it. */
export function autoApproves(
  client: ClientRow,
  snapshot: Snapshot,
  preferences: Record<string, SourcePreferences>,
) {
  return (
    uses(client, "answer", snapshot, preferences) &&
    switchedOn(snapshot.settings, "autoApprove", client.id)
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
      return liveOn(client, snapshot, preferences);
    case "answer":
      return switchedOn(settings, "answer", client.id);
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
