/* One client's settings: its switches, its approvals where it has a switch
 * of its own, Agent HUD's hooks in it, and its agent's accounts: in full when
 * the client is its vendor's only one, else as a list that leads to the
 * vendor's page, where they are set. */
import { RefreshCw } from "lucide-react";
import type {
  ClientRow,
  NativeMethod,
  PanelLayout,
  Provider,
  Snapshot,
  SourcePreferences,
} from "@agent-hud/protocol";
import { Button } from "@agent-hud/ui/components/button";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@agent-hud/ui/components/tooltip";
import { HUDArtwork as HUDLogo } from "@agent-hud/hud/HUDArtwork";
import { LinkButton, ToggleRow } from "../SettingsControls";
import { Notice, Page, Section } from "../SettingsPrimitives";
import type { Translate } from "@agent-hud/ui/format";
import { AgentAccounts } from "./AgentAccounts";
import { ClientAccounts, ClientHooks } from "./ClientSections";
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
  /** Opens the vendor's page; none for a vendor of this one client, whose
   * accounts this page shows itself. */
  openGroup?: () => void;
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
  openGroup,
}: AgentDetailProps) {
  const { pending, error, setError, perform } = usePerform();

  const enabled = client.enabled;
  // The clients whose approval hook goes in only once the user turns it on.
  const optInApprovals = snapshot.settings.optInApprovals ?? [];
  const live = preferences[client.id]?.live !== false;
  const reminders = preferences[client.id]?.reminders !== false;
  const offReason = t("Turn on Enabled first.");
  const refreshLabel = t("Read all agents again");

  return (
    <Page>
      <header className="agents-detail-header">
        <HUDLogo source={client.agent} size={24} />
        <div className="agents-detail-title">
          <h2>{client.name}</h2>
          {!client.detected && <p>{t("Not on this computer")}</p>}
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label={refreshLabel}
              disabled={pending}
              onClick={() => void perform(() => action("refresh"))}
            >
              <RefreshCw size={14} />
            </Button>
          </TooltipTrigger>
          <TooltipContent>{refreshLabel}</TooltipContent>
        </Tooltip>
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
      <Section footer={source.notice ?? undefined}>
        <ToggleRow
          title={t("Enabled")}
          checked={enabled}
          disabled={pending}
          change={(value) =>
            void perform(() =>
              action("set_provider", { id: client.id, enabled: value }),
            )
          }
        />
        <ToggleRow
          title={t("Live status")}
          subtitle={t("Running and waiting sessions on the HUD.")}
          checked={live}
          disabled={pending || !enabled}
          reason={!enabled ? offReason : undefined}
          change={(value) =>
            void perform(() => onPreferences(client.id, { live: value }))
          }
        />
        <ToggleRow
          title={t("Completion reminders")}
          checked={live && reminders}
          disabled={pending || !enabled || !live}
          reason={
            !enabled ? offReason : !live ? t("Needs Live status.") : undefined
          }
          change={(value) =>
            void perform(() => onPreferences(client.id, { reminders: value }))
          }
        />
      </Section>
      {source.id === "copilot" && (
        <Section
          footer={
            snapshot.settings.clientHooks
              ? undefined
              : t("Takes effect once client hooks are on in Agent plugins.")
          }
        >
          <ToggleRow
            title={t("Answer permission requests in the HUD")}
            subtitle={t(
              "Copilot then asks Agent HUD before its own rules and this session's approvals, so every tool call it would run or ask about waits for your answer in the HUD. Runs started with -p, --allow-all-tools, --allow-all or --yolo stay with Copilot.",
            )}
            checked={optInApprovals.includes(source.id)}
            disabled={pending || !enabled}
            reason={!enabled ? offReason : undefined}
            change={(value) =>
              void perform(() =>
                action("set_setting", {
                  key: "optInApprovals",
                  value: [
                    ...optInApprovals.filter((id) => id !== source.id),
                    ...(value ? [source.id] : []),
                  ],
                }),
              )
            }
          />
        </Section>
      )}
      <ClientHooks t={t} snapshot={snapshot} source={source} />
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
