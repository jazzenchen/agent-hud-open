// One folding card per client: its logo and name, a pill for each thing Agent
// HUD adds to it, and inside, the plugin row with its switches and the HUD
// tools row. A card opens by itself only when something in it needs you.
import { Fragment, useState, type ReactNode } from "react";
import { Check } from "lucide-react";
import { HUDArtwork } from "@agent-hud/hud/HUDArtwork";
import type { Snapshot } from "@agent-hud/protocol";
import { mcpRow, pluginRow, type RowState } from "../../lib/agent-plugins";
import {
  Collapsible,
  Pill,
  Row,
  SmallButton,
  type SettingsPaneProps,
  type Text,
} from "../SettingsPrimitives";
import { PluginControl, StatePill, StatusRow } from "./rows";
import type { AgentPluginContext, AgentPluginItem } from "./types";
import {
  useBriefly,
  type HudTools,
  type McpClients,
  type PluginStatus,
} from "./use-status";
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
  t: Text,
) {
  const entry = clients.status?.clients.find((item) => item.id === client.id);
  const state = mcpRow({
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
      status: ["Off", "已关闭"],
      action: null,
    } satisfies RowState,
    reason: tools.state.unavailable
      ? t(...tools.state.unavailable)
      : t(
          "Turn on “Let agents arrange the HUD” under Agent tools.",
          "在下方「智能体工具」里打开「允许智能体调整 HUD」。",
        ),
  };
}

function ClaudeAnswers({
  snapshot,
  action,
  t,
}: {
  snapshot: Snapshot;
  action: SettingsPaneProps["action"];
  t: Text;
}) {
  const [pending, setPending] = useState(false);
  return (
    <PluginControl
      title={t("Answer questions", "回答提问")}
      checked={snapshot.settings.claudeAnswers !== false}
      pending={pending}
      onChange={(value) => {
        setPending(true);
        void action("set_setting", { key: "claudeAnswers", value })
          .catch(() => {})
          .finally(() => setPending(false));
      }}
    />
  );
}

function ClientCard({
  name,
  logo,
  light,
  missing,
  summary,
  defaultOpen,
  children,
}: {
  name: string;
  logo?: string;
  light: boolean;
  missing: boolean;
  summary: ReactNode;
  defaultOpen: boolean;
  children: ReactNode;
}) {
  const muted = { color: "var(--muted-foreground)" };
  return (
    <Collapsible
      icon={
        logo && (
          <span style={{ display: "inline-flex", opacity: missing ? 0.5 : 1 }}>
            <HUDArtwork source={logo} size={18} light={light} />
          </span>
        )
      }
      title={missing ? <span style={muted}>{name}</span> : name}
      summary={summary}
      defaultOpen={defaultOpen}
    >
      {children}
    </Collapsible>
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
  t: Text;
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
            {t("Not found", "未发现")}
          </Pill>
        ) : (
          <>
            {pluginState && <StatePill state={pluginState} t={t} />}
            {answersOff && (
              <Pill tone="orange" dot>
                {t("Answers off", "回答已关闭")}
              </Pill>
            )}
            <StatePill state={hud.state} t={t} />
          </>
        )
      }
    >
      {plugin && pluginState && (
        <StatusRow
          title={t("Agent HUD plugin", "Agent HUD 插件")}
          subtitle={
            client.id === "claudeCode"
              ? t(
                  "Answer Claude's questions in the HUD.",
                  "在 HUD 上回答 Claude 的提问。",
                )
              : t(
                  "Answer its questions and approvals in the HUD.",
                  "在 HUD 上回答它的提问和批准请求。",
                )
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
        <ClaudeAnswers snapshot={snapshot} action={action} t={t} />
      )}
      {installed === true &&
        switches.map((item) => (
          <Fragment key={item.id}>{item.render(context)}</Fragment>
        ))}
      {own.map((item) => (
        <Fragment key={item.id}>{item.render(context)}</Fragment>
      ))}
      <StatusRow
        title={t("HUD tools", "HUD 工具")}
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

/** A card for a client only a product build knows, holding its rows. */
export function ProductClientCard({
  name,
  items,
  context,
  light,
}: {
  name: string;
  items: AgentPluginItem[];
  context: AgentPluginContext;
  light: boolean;
}) {
  return (
    <ClientCard
      name={name}
      light={light}
      missing={false}
      summary={null}
      defaultOpen={false}
    >
      {items.map((item) => (
        <Fragment key={item.id}>{item.render(context)}</Fragment>
      ))}
    </ClientCard>
  );
}

/** HUD tools for any other agent: the settings to paste by hand. */
export function OtherAgentsCard({
  json,
  icon,
  t,
}: {
  json: unknown;
  icon: ReactNode;
  t: Text;
}) {
  const [copied, setCopied] = useBriefly<true>(2000);
  return (
    <Collapsible icon={icon} title={t("Other agents", "其他智能体")}>
      <Row
        title={t("HUD tools", "HUD 工具")}
        tag="MCP"
        subtitle={t(
          "Paste into the agent's MCP settings.",
          "粘贴到智能体的 MCP 设置里。",
        )}
      >
        <SmallButton
          disabled={json == null}
          onClick={() =>
            void navigator.clipboard
              ?.writeText(JSON.stringify(json, null, 2))
              .then(() => setCopied(true))
              .catch(() => {})
          }
        >
          {copied ? (
            <>
              <Check size={13} aria-hidden />
              {t("Copied", "已复制")}
            </>
          ) : (
            t("Copy settings", "复制设置")
          )}
        </SmallButton>
      </Row>
    </Collapsible>
  );
}
