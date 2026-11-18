/* Settings → Agents: what Agent HUD does, one switch each for every agent,
 * then the agents, each with the features it uses; at the bottom, HUD tools
 * for any other agent and the details behind hooks and tools. An agent opens
 * its own page, where any feature can be turned off for it alone and its
 * plugin and HUD tools are added; a vendor's name opens the vendor's page,
 * where its agents' accounts are set. */
import { useLayoutEffect, useRef, useState } from "react";
import { Braces } from "lucide-react";
import type {
  ClientRow,
  NativeMethod,
  PanelLayout,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import { useT } from "@agent-hud/ui/format-context";
import { Page } from "../SettingsPrimitives";
import type { AgentPluginItem } from "../agent-plugins/types";
import { OtherAgentsCard } from "../agent-plugins/OtherAgentsCard";
import { DetailsSection } from "../agent-plugins/DetailsSection";
import { nativeClients } from "../agent-plugins/services";
import { ToolsSection } from "../agent-plugins/ToolsSection";
import { useHudTools, useMcpClients } from "../agent-plugins/use-status";
import { AgentList } from "./AgentList";
import { AgentDetail } from "./AgentDetail";
import { FeaturesSection } from "./FeaturesSection";
import { GroupDetail } from "./GroupDetail";
import "./agents.css";

export interface AgentsPageProps {
  snapshot: Snapshot;
  /** Whether the window is on screen; Codex's model list reads only then. */
  active?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
  /** A product build's own agent tools, shown with Agent HUD's, and its
   * rows for a client, on the client's page. */
  items?: AgentPluginItem[];
  request: (method: string, params?: unknown) => Promise<unknown>;
  /** The client whose page opens first (a link to it); else the list. */
  open?: string | null;
}

type View = { kind: "list" } | { kind: "client" | "group"; id: string };

export default function AgentsPage({
  snapshot,
  active,
  action,
  layout,
  onLayout,
  preferences,
  onPreferences,
  items = [],
  request,
  open,
}: AgentsPageProps) {
  const t = useT();
  const [view, setView] = useState<View>(
    open ? { kind: "client", id: open } : { kind: "list" },
  );
  const page = useRef<HTMLDivElement>(null);
  const tools = useHudTools(snapshot, action);
  // Which clients have HUD tools: the list's tags and each agent's row.
  const mcp = useMcpClients(nativeClients, active ?? true, t);
  // The order the runtime lists them in, so the list never jumps.
  const clients: ClientRow[] = snapshot.clients?.length
    ? snapshot.clients
    : snapshot.providers.map((provider) => ({
        id: provider.id,
        agent: provider.id,
        name: provider.name,
        group: provider.id,
        groupName: "",
        detected: provider.detected,
        enabled: provider.enabled,
      }));
  const members = (group: string) =>
    clients.filter((row) => group && row.group === group);
  // Where the list was scrolled to when an agent or vendor was opened, so
  // coming back lands there; null until a page is opened from it.
  const listTop = useRef<number | null>(null);
  const scroller = () => page.current?.closest(".settings-scroll");
  const go = (next: View) => {
    if (view.kind === "list") listTop.current = scroller()?.scrollTop ?? 0;
    setView(next);
  };
  // An agent's or vendor's page starts at its top; the list, where it was
  // left. Set once the page is drawn, so the list is tall enough again.
  useLayoutEffect(() => {
    if (listTop.current === null) return;
    scroller()?.scrollTo({ top: view.kind === "list" ? listTop.current : 0 });
  }, [view]);
  const back = () => go({ kind: "list" });
  const client =
    view.kind === "client"
      ? clients.find((row) => row.id === view.id)
      : undefined;
  const source = snapshot.providers.find(
    (provider) => provider.id === client?.agent,
  );
  const group = view.kind === "group" ? members(view.id) : [];

  return (
    <div className="agents-page" ref={page}>
      {client && source ? (
        // Keyed by client, so a draft or an error never carries over.
        <AgentDetail
          key={client.id}
          t={t}
          snapshot={snapshot}
          client={client}
          source={source}
          active={active}
          action={action}
          layout={layout}
          onLayout={onLayout}
          preferences={preferences}
          onPreferences={onPreferences}
          back={back}
          openGroup={
            members(client.group).length > 1 && client.groupName
              ? () => go({ kind: "group", id: client.group })
              : undefined
          }
          clients={mcp}
          tools={tools}
          items={items.filter((item) => item.place !== "tools")}
          request={request}
        />
      ) : group.length ? (
        <GroupDetail
          key={`group:${group[0].group}`}
          t={t}
          snapshot={snapshot}
          clients={group}
          active={active}
          action={action}
          layout={layout}
          onLayout={onLayout}
          back={back}
        />
      ) : (
        <Page>
          <FeaturesSection t={t} snapshot={snapshot} action={action} />
          <ToolsSection
            tools={tools}
            groups={items.filter((item) => item.place === "tools")}
            snapshot={snapshot}
            action={action}
            request={request}
            active={active ?? true}
            t={t}
          />
          <AgentList
            t={t}
            snapshot={snapshot}
            clients={clients}
            preferences={preferences}
            select={(id) => go({ kind: "client", id })}
            selectGroup={(id) => go({ kind: "group", id })}
            added={mcp.status}
          />
          <OtherAgentsCard
            json={mcp.status?.json}
            icon={<Braces size={16} strokeWidth={1.8} />}
            t={t}
          />
          <DetailsSection
            snapshot={snapshot}
            clients={mcp.status}
            tools={tools}
            t={t}
          />
        </Page>
      )}
    </div>
  );
}
