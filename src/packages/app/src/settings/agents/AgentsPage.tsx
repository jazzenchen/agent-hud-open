/* Settings → Agents: the clients on the left, grouped by vendor, the chosen
 * one's settings on the right. */
import { useRef, useState } from "react";
import type {
  ClientRow,
  NativeMethod,
  PanelLayout,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import { useT } from "@agent-hud/ui/format-context";
import { AgentList } from "./AgentList";
import { AgentDetail } from "./AgentDetail";
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
}

export default function AgentsPage({
  snapshot,
  active,
  action,
  layout,
  onLayout,
  preferences,
  onPreferences,
}: AgentsPageProps) {
  const t = useT();
  const [selectedID, setSelected] = useState<string>();
  const page = useRef<HTMLDivElement>(null);
  // The order the runtime lists them in, so the list never jumps when the
  // panel order changes; the list puts found ones first.
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
  const client =
    clients.find((row) => row.id === selectedID) ??
    clients.find((row) => row.detected) ??
    clients[0];
  // The agent whose data the client shares: its account, panel and relays.
  const source = snapshot.providers.find(
    (provider) => provider.id === client?.agent,
  );
  const select = (id: string) => {
    setSelected(id);
    // Show the new agent from its top, wherever the last one was scrolled.
    page.current?.closest(".settings-scroll")?.scrollTo({ top: 0 });
  };

  return (
    <div className="agents-page" ref={page}>
      <AgentList
        t={t}
        clients={clients}
        selected={client?.id}
        select={select}
      />
      <div className="agents-detail">
        {client && source && (
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
          />
        )}
      </div>
    </div>
  );
}
