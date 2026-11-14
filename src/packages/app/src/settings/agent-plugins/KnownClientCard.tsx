// A known client's card: its pills while closed, and inside, Agent HUD's
// plugin row with its switches and the HUD tools row. It opens by itself only
// when something in it needs you.
import { Fragment, useState } from "react";
import { toast } from "sonner";
import type { Snapshot } from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { mcpRow, pluginRow, type RowState } from "../../lib/agent-plugins";
import { Pill, type SettingsPaneProps } from "../SettingsPrimitives";
import { ClientCard } from "./ClientCards";
import { PluginControl, StatePill, StatusRow } from "./rows";
import type { AgentPluginContext, AgentPluginItem } from "./types";
import type { HudTools, McpClients, PluginStatus } from "./use-status";
import type { knownClients } from "./services";

type Client = (typeof knownClients)[number];

/** Whether a row waits on the user: something to install or add that can be. */
function needsYou(state: RowState) {
  return (
    state.tone === "off" &&
    state.action !== null &&
    !state.action.unavailable &&
    !state.busy
  );
}

/** The HUD tools row in a client's card. While the app-wide switch is off it
 * says "Off" and points at the Agent tools section, which holds the switch,
 * instead of offering its own. */
function hudToolsState(
  client: Client,
  clients: McpClients,
  tools: HudTools,
  t: Translate,
) {
  const entry = clients.status?.clients.find((item) => item.id === client.id);
  const state = mcpRow(t, {
    client: client.name,
    setupReady: clients.status ? clients.status.ready : null,
    clientFound: entry ? entry.available : true,
    added: Boolean(entry?.installed),
    hasCommand: Boolean(entry?.command),
    toolsOn: tools.on,
    toolsUnavailable: tools.state.unavailable,
    listening: tools.listening,
    running: clients.running[client.id] ?? null,
    justAdded: clients.justAdded.includes(client.id),
    copied: clients.copied === client.id,
  });
  if (state.action?.id !== "turn-on") return { state, reason: null };
  return {
    state: {
      ...state,
      tone: "off",
      status: t("Off"),
      action: null,
    } satisfies RowState,
    reason:
      tools.state.unavailable ??
      t("Turn on “Let agents arrange the HUD” under Agent tools."),
  };
}

/** One of the plugin's switches that is an app setting: "Answer questions",
 * "Send messages". */
function SettingControl({
  title,
  description,
  setting,
  snapshot,
  action,
}: {
  title: string;
  description?: string;
  setting: "claudeAnswers" | "claudeMessages" | "deepseekMessages";
  snapshot: Snapshot;
  action: SettingsPaneProps["action"];
}) {
  const [pending, setPending] = useState(false);
  return (
    <PluginControl
      title={title}
      description={description}
      checked={snapshot.settings[setting] !== false}
      pending={pending}
      onChange={(value) => {
        setPending(true);
        // The row has no place for a failure, so it shows as a toast.
        void action("set_setting", { key: setting, value })
          .catch((failure) => toast.error(String(failure)))
          .finally(() => setPending(false));
      }}
    />
  );
}
/** A known client's card: Agent HUD's plugin for it (with its switches once
 * installed), a product's own rows, and HUD tools. */
export function KnownClientCard({
  client,
  plugin,
  clients,
  tools,
  items,
  snapshot,
  action,
  request,
  active,
  light,
  t,
}: {
  client: Client;
  plugin: PluginStatus | null;
  clients: McpClients;
  tools: HudTools;
  items: AgentPluginItem[];
  snapshot: Snapshot;
  action: SettingsPaneProps["action"];
  request: AgentPluginContext["request"];
  active: boolean;
  light: boolean;
  t: Translate;
}) {
  const entry = clients.status?.clients.find((item) => item.id === client.id);
  const found = entry ? entry.available : null;
  const missing = found === false;
  const installed = plugin?.status ? plugin.status.installed : null;
  const context: AgentPluginContext = {
    snapshot,
    request,
    active,
    clientFound: found,
    plugin: { installed },
  };
  const pluginState =
    plugin &&
    pluginRow(
      t,
      {
        installed,
        sessions: plugin.status?.sessions ?? 0,
        clientFound: plugin.status ? plugin.status.found : found,
        running: plugin.running,
        justInstalled: plugin.justInstalled,
      },
      client.name,
      client.plugin?.live,
    );
  const hud = hudToolsState(client, clients, tools, t);
  const error = clients.errors[client.id];
  const switches = items.filter((item) => item.place === "plugin");
  const own = items.filter((item) => (item.place ?? "row") === "row");
  // Read once the statuses are in, so the card opens only when it should.
  const loaded =
    clients.status !== null &&
    (!plugin || plugin.status !== null || plugin.error !== null);
  // Answering is off: the HUD's "Open Agent plugins" fix lands here for it,
  // so the card opens on the switch and says so while closed.
  const answersOff =
    client.id === "claudeCode" &&
    installed === true &&
    snapshot.settings.claudeAnswers === false;
  const open =
    !missing &&
    ((pluginState !== null && needsYou(pluginState)) ||
      needsYou(hud.state) ||
      answersOff ||
      Boolean(plugin?.error || error));
  return (
    <ClientCard
      key={loaded ? "loaded" : "reading"}
      name={client.name}
      logo={client.logo}
      light={light}
      missing={missing}
      defaultOpen={loaded && open}
      summary={
        missing ? (
          <Pill tone="gray" dot>
            {t("Not found")}
          </Pill>
        ) : (
          <>
            {pluginState && (
              <StatePill state={pluginState} label={t("Plugin")} t={t} />
            )}
            {answersOff && (
              <Pill tone="orange" dot>
                {t("Answers off")}
              </Pill>
            )}
            <StatePill state={hud.state} label={t("HUD tools")} t={t} />
          </>
        )
      }
    >
      {plugin && pluginState && (
        <StatusRow
          title={t("Agent HUD plugin")}
          subtitle={
            client.id === "claudeCode"
              ? t("Answer Claude's questions in the HUD.")
              : t("Answer its questions and approvals in the HUD.")
          }
          state={pluginState}
          error={plugin.error}
          t={t}
          onAction={(id) => {
            if (id === "install" || id === "remove") void plugin.change(id);
          }}
        />
      )}
      {installed === true && client.id === "claudeCode" && (
        <>
          <SettingControl
            title={t("Answer questions")}
            setting="claudeAnswers"
            snapshot={snapshot}
            action={action}
          />
          <SettingControl
            title={t("Send messages")}
            description={t("From Agent HUD into open sessions.")}
            setting="claudeMessages"
            snapshot={snapshot}
            action={action}
          />
        </>
      )}
      {installed === true && client.id === "deepseek" && (
        <SettingControl
          title={t("Send messages")}
          description={t("Queued for open sessions; never interrupts a turn.")}
          setting="deepseekMessages"
          snapshot={snapshot}
          action={action}
        />
      )}
      {installed === true &&
        switches.map((item) => (
          <Fragment key={item.id}>{item.render(context)}</Fragment>
        ))}
      {own.map((item) => (
        <Fragment key={item.id}>{item.render(context)}</Fragment>
      ))}
      <StatusRow
        title={t("HUD tools")}
        tag="MCP"
        state={hud.state}
        pillReason={hud.reason}
        error={error}
        t={t}
        onAction={(id) => {
          if (id === "add" || id === "remove")
            void clients.change(client.id, id);
          else if (id === "copy" && entry?.command)
            clients.copyCommand(client.id, entry.command);
        }}
      />
    </ClientCard>
  );
}
