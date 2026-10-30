/* Settings → Agents: the list of agents on the left, the chosen one's
 * settings on the right. */
import { useRef, useState } from "react";
import type {
  NativeMethod,
  PanelLayout,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import { useText } from "../SettingsPrimitives";
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
  const t = useText(snapshot);
  const [selectedID, setSelected] = useState<string>();
  const page = useRef<HTMLDivElement>(null);
  // Found agents first; otherwise the order the runtime lists them in, so
  // the list never jumps when the panel order changes.
  const providers = [...snapshot.providers].sort(
    (a, b) => Number(b.detected) - Number(a.detected),
  );
  const source =
    providers.find((provider) => provider.id === selectedID) ?? providers[0];
  const select = (id: string) => {
    setSelected(id);
    // Show the new agent from its top, wherever the last one was scrolled.
    page.current?.closest(".settings-scroll")?.scrollTo({ top: 0 });
  };

  return (
    <div className="agents-page" ref={page}>
      <AgentList
        t={t}
        providers={providers}
        selected={source?.id}
        select={select}
      />
      <div className="agents-detail">
        {source && (
          // Keyed by agent, so a draft or an error never carries over.
          <AgentDetail
            key={source.id}
            t={t}
            snapshot={snapshot}
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
