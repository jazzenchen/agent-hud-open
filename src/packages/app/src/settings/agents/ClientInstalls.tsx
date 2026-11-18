/* What Agent HUD puts into a client, shown on its page among the features
 * they carry: Agent HUD's plugin where the client has one (Claude Code and
 * DeepSeek Harness, which take answers and messages through it), and HUD
 * tools, each with the button that adds or removes it. A command line and a
 * desktop app that share one setup (Claude Code's, Codex's) show the same. */
import { Fragment, type ReactNode } from "react";
import type { ClientRow, Snapshot } from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import { mcpRow, pluginRow, type RowState } from "../../lib/agent-plugins";
import { StatusRow } from "../agent-plugins/rows";
import { nativeDeepSeekPlugin, nativePlugin } from "../agent-plugins/services";
import type { AgentPluginItem } from "../agent-plugins/types";
import {
  usePlugin,
  type HudTools,
  type McpClients,
} from "../agent-plugins/use-status";
import { toolsClient } from "./features";

/** Why no client can be given HUD tools here. */
function setupNote(snapshot: Snapshot, t: Translate) {
  const platform =
    snapshot.connection.platformCapabilities?.os ??
    snapshot.connection.platform;
  return platform === "windows"
    ? t("Adding HUD tools needs Agent HUD run from where its installer put it.")
    : platform === "macos"
      ? t("Adding HUD tools needs Agent HUD in an approved macOS location.")
      : t("This build can't add HUD tools.");
}

/** The HUD tools row's state. While the app-wide switch is off it says "Off"
 * and points at Agent tools on the Agents page, which holds the switch. */
function toolsState(
  id: string,
  name: string,
  clients: McpClients,
  tools: HudTools,
  t: Translate,
) {
  const entry = clients.status?.clients.find((item) => item.id === id);
  const state = mcpRow(t, {
    client: name,
    setupReady: clients.status ? clients.status.ready : null,
    clientFound: entry ? entry.available : true,
    added: Boolean(entry?.installed),
    hasCommand: Boolean(entry?.command),
    toolsOn: tools.on,
    toolsUnavailable: tools.state.unavailable,
    listening: tools.listening,
    running: clients.running[id] ?? null,
    justAdded: clients.justAdded.includes(id),
    copied: clients.copied === id,
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

/** The client's plugin row, the rows a product build adds, and its HUD
 * tools row, for its page to place among the features; with why HUD tools
 * can't be added on this computer, when they can't. */
export function useClientInstalls({
  client,
  snapshot,
  clients,
  tools,
  items,
  request,
  active,
  t,
}: {
  client: ClientRow;
  snapshot: Snapshot;
  clients: McpClients;
  tools: HudTools;
  items: AgentPluginItem[];
  request: (method: string, params?: unknown) => Promise<unknown>;
  active: boolean;
  t: Translate;
}): {
  plugin: ReactNode;
  own: ReactNode;
  tools: ReactNode;
  note: string | null;
} {
  const known = toolsClient(client);
  const id = known?.id ?? "";
  const plugin = usePlugin(
    id,
    id === "deepseek" ? nativeDeepSeekPlugin : nativePlugin,
    active && Boolean(known?.plugin),
    t,
  );
  const entry = clients.status?.clients.find((item) => item.id === id);
  const installed = plugin.status ? plugin.status.installed : null;
  const context = {
    snapshot,
    request,
    active,
    clientFound: entry ? entry.available : null,
    plugin: { installed },
  };
  const mine = items.filter((item) => item.client === id);
  const render = (list: AgentPluginItem[]) =>
    list.map((item) => (
      <Fragment key={item.id}>{item.render(context)}</Fragment>
    ));
  const pluginRowState =
    known?.plugin &&
    pluginRow(
      t,
      {
        installed,
        sessions: plugin.status?.sessions ?? 0,
        clientFound: plugin.status
          ? plugin.status.found
          : (entry?.available ?? null),
        running: plugin.running,
        justInstalled: plugin.justInstalled,
      },
      known.name,
      known.plugin.live,
    );
  const hud = known && toolsState(id, known.name, clients, tools, t);
  const error = clients.errors[id];
  return {
    plugin: pluginRowState && (
      <>
        <StatusRow
          title={t("Agent HUD plugin")}
          subtitle={
            id === "claudeCode"
              ? t("Answer Claude's questions in the HUD.")
              : t("Answer its questions and approvals in the HUD.")
          }
          state={pluginRowState}
          error={plugin.error}
          t={t}
          onAction={(action) => {
            if (action === "install" || action === "remove")
              void plugin.change(action);
          }}
        />
        {installed === true &&
          render(mine.filter((item) => item.place === "plugin"))}
      </>
    ),
    own: render(mine.filter((item) => (item.place ?? "row") === "row")),
    tools: hud && (
      <StatusRow
        title={t("HUD tools")}
        tag="MCP"
        state={hud.state}
        pillReason={hud.reason}
        error={error}
        t={t}
        onAction={(action) => {
          if (action === "add" || action === "remove")
            void clients.change(id, action);
          else if (action === "copy" && entry?.command)
            clients.copyCommand(id, entry.command);
        }}
      />
    ),
    note:
      known && clients.status?.ready === false ? setupNote(snapshot, t) : null,
  };
}
