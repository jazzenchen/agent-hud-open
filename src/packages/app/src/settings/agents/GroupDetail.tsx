/* A vendor's page: the accounts of each of its agents, which its clients
 * share. A vendor whose clients are one agent's (Claude Code CLI and Claude
 * Desktop) shows that agent's accounts; one with several agents (Qoder,
 * Qoder CN, QoderWork, Qwen Code) shows each under the agent's name. */
import type {
  ClientRow,
  NativeMethod,
  PanelLayout,
  Snapshot,
} from "@agent-hud/protocol";
import { ChevronLeft } from "lucide-react";
import { AgentLogo } from "../AgentLogo";
import type { Translate } from "@agent-hud/ui/format";
import { LinkButton } from "../SettingsControls";
import { Notice, Page } from "../SettingsPrimitives";
import { AgentAccounts } from "./AgentAccounts";
import { usePerform } from "./use-perform";

export function GroupDetail({
  t,
  snapshot,
  clients,
  active,
  action,
  layout,
  onLayout,
  back,
}: {
  t: Translate;
  snapshot: Snapshot;
  /** The vendor's clients, in the list's order. */
  clients: ClientRow[];
  active?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  /** Back to the list of agents. */
  back: () => void;
}) {
  const { pending, error, setError, perform } = usePerform();
  const agents = [...new Set(clients.map((client) => client.agent))]
    .map((id) => snapshot.providers.find((provider) => provider.id === id))
    .filter((provider) => provider !== undefined);
  return (
    <Page>
      <div className="agents-back">
        <LinkButton onClick={back}>
          <ChevronLeft size={13} />
          {t("Agents")}
        </LinkButton>
      </div>
      <header className="agents-detail-header">
        <AgentLogo source={clients[0]?.agent ?? ""} size={24} />
        <div className="agents-detail-title">
          <h2>{clients[0]?.groupName}</h2>
        </div>
      </header>
      {error && (
        <Notice
          tone="error"
          action={
            <LinkButton onClick={() => setError(null)}>
              {t("Dismiss")}
            </LinkButton>
          }
        >
          {error}
        </Notice>
      )}
      {agents.map((source) => (
        <div key={source.id} className="agents-group-agent">
          {agents.length > 1 && (
            <h3 className="agents-group-agent-name">
              <AgentLogo source={source.id} size={16} />
              {source.name}
            </h3>
          )}
          <AgentAccounts
            t={t}
            snapshot={snapshot}
            source={source}
            active={active}
            action={action}
            layout={layout}
            onLayout={onLayout}
            perform={perform}
            pending={pending}
          />
        </div>
      ))}
    </Page>
  );
}
