/* One agent's own page: whether Agent HUD follows it, the step it still
 * waits for, the features it uses (each can be turned off for it alone) with
 * what Agent HUD puts into it (its plugin, HUD tools), and its accounts: in full when it is its vendor's only client, else as a list
 * that leads to the vendor's page, where they are set. */
import { ChevronLeft, RefreshCw } from "lucide-react";
import type {
  ClientRow,
  NativeMethod,
  PanelLayout,
  Provider,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import { AgentLogo } from "../AgentLogo";
import { LinkButton, ToggleRow } from "../SettingsControls";
import { Notice, Page, Section } from "../SettingsPrimitives";
import type { Translate } from "@agent-hud/ui/format";
import { AgentAccounts } from "./AgentAccounts";
import { ClientAccounts } from "./ClientSections";
import { ClientFeatures } from "./ClientFeatures";
import { useClientInstalls } from "./ClientInstalls";
import type { AgentPluginItem } from "../agent-plugins/types";
import type { HudTools, McpClients } from "../agent-plugins/use-status";
import { hookStep } from "./steps";
import { usePerform } from "./use-perform";

export interface AgentDetailProps {
  t: Translate;
  snapshot: Snapshot;
  /** The client this page sets. */
  client: ClientRow;
  /** The agent whose data the client shares. */
  source: Provider;
  active?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
  /** Back to the list of agents. */
  back: () => void;
  /** Opens the vendor's page; none for a vendor of this one client, whose
   * accounts this page shows itself. */
  openGroup?: () => void;
  /** Which clients have HUD tools, and the app-wide switch for them. */
  clients: McpClients;
  tools: HudTools;
  /** A product build's own rows for this client. */
  items: AgentPluginItem[];
  request: (method: string, params?: unknown) => Promise<unknown>;
}

export function AgentDetail({
  t,
  snapshot,
  client,
  source,
  active,
  action,
  layout,
  onLayout,
  preferences,
  onPreferences,
  back,
  openGroup,
  clients,
  tools,
  items,
  request,
}: AgentDetailProps) {
  const { pending, error, setError, perform } = usePerform();
  const installs = useClientInstalls({
    client,
    snapshot,
    clients,
    tools,
    items,
    request,
    active: active ?? true,
    t,
  });
  // The step the agent wants before it runs Agent HUD's hooks, such as
  // Codex's review, or that it gets none on this system yet: a warning while
  // the agent says it waits for it, else a line under its switch.
  const step = hookStep(snapshot, source.id);
  const footer =
    [source.notice, step && !step.waiting ? step.text : null]
      .filter(Boolean)
      .join(" ") || undefined;
  const refreshLabel = t("Read all agents again");

  return (
    <Page>
      <div className="agents-back">
        <LinkButton onClick={back}>
          <ChevronLeft size={13} />
          {t("Agents")}
        </LinkButton>
      </div>
      <header className="agents-detail-header">
        <AgentLogo source={client.agent} size={24} />
        <div className="agents-detail-title">
          <h2>{client.name}</h2>
          {!client.detected && <p>{t("Not on this computer")}</p>}
        </div>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={refreshLabel}
          title={refreshLabel}
          disabled={pending}
          onClick={() => void perform(() => action("refresh"))}
        >
          <RefreshCw size={14} />
        </Button>
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
      {client.enabled && step?.waiting && <Notice>{step.text}</Notice>}
      <Section footer={footer}>
        <ToggleRow
          title={t("Enabled")}
          checked={client.enabled}
          disabled={pending}
          change={(value) =>
            void perform(() =>
              action("set_provider", { id: client.id, enabled: value }),
            )
          }
        />
      </Section>
      <ClientFeatures
        t={t}
        snapshot={snapshot}
        client={client}
        action={action}
        preferences={preferences}
        onPreferences={onPreferences}
        perform={perform}
        pending={pending}
        installs={installs}
      />
      {openGroup ? (
        <ClientAccounts
          t={t}
          snapshot={snapshot}
          client={client}
          openGroup={openGroup}
        />
      ) : (
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
      )}
    </Page>
  );
}
