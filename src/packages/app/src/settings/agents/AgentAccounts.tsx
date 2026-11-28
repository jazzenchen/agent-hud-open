/* One agent's accounts: where they come from, what the panel shows of them,
 * the relays they go through and the API credentials they are read with. An
 * agent's account is shared by its clients, so this sits on its vendor's
 * page when the vendor has several clients, and on the client's own page
 * otherwise. */
import type {
  NativeMethod,
  PanelLayout,
  Provider,
  Snapshot,
} from "@agent-hud/protocol";
import type { Translate } from "@agent-hud/ui/format";
import AntigravityAccounts from "../AntigravityAccounts";
import { CodexModelSettings } from "../CodexModelSettings";
import ManagedAccountProfiles from "../ManagedAccountProfiles";
import { Collapsible } from "../SettingsPrimitives";
import { PanelBlocks } from "./PanelBlocks";
import { AgentQuotaReading } from "./AgentQuotaReading";
import { AgentRelays } from "./AgentRelays";
import { AgentWebSignIn } from "./AgentWebSignIn";
import { Connection } from "./Connection";
import { CopilotQuota } from "./CopilotQuota";
import type { Perform } from "./use-perform";

export function AgentAccounts({
  t,
  snapshot,
  source,
  active,
  action,
  layout,
  onLayout,
  perform,
  pending,
}: {
  t: Translate;
  snapshot: Snapshot;
  source: Provider;
  active?: boolean;
  action: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
  layout: PanelLayout;
  onLayout: (layout: PanelLayout) => Promise<unknown>;
  perform: Perform;
  pending: boolean;
}) {
  return (
    // The agent's data is read while any of its clients is on.
    <div
      className={`agents-detail-rest${source.enabled ? "" : " is-off"}`}
      inert={!source.enabled}
    >
      {source.id === "codex" && (
        <ManagedAccountProfiles
          t={t}
          provider="codex"
          settingKey="codexHomes"
          homes={snapshot.settings.codexHomes ?? []}
          snapshot={snapshot}
          action={action}
        />
      )}
      {source.id === "claude" && (
        <ManagedAccountProfiles
          t={t}
          provider="claude"
          settingKey="claudeHomes"
          homes={snapshot.settings.claudeHomes ?? []}
          snapshot={snapshot}
          action={action}
        />
      )}
      {source.id === "antigravity" && (
        <>
          <ManagedAccountProfiles
            t={t}
            provider="antigravity"
            settingKey="antigravityHomes"
            homes={snapshot.settings.antigravityHomes ?? []}
            snapshot={snapshot}
            action={action}
          />
          <Collapsible
            title={t("CLI status-line hook (advanced)")}
            subtitle={t(
              "Optional status-line callback for external Antigravity CLI settings files.",
            )}
          >
            <AntigravityAccounts
              t={t}
              settings={snapshot.settings.antigravityCliSettings ?? []}
              action={action}
            />
          </Collapsible>
        </>
      )}
      {source.id === "copilot" && (
        <CopilotQuota
          t={t}
          snapshot={snapshot}
          action={action}
          perform={perform}
          pending={pending}
        />
      )}
      <AgentQuotaReading
        t={t}
        snapshot={snapshot}
        source={source}
        action={action}
        perform={perform}
        pending={pending}
      />
      <PanelBlocks
        t={t}
        snapshot={snapshot}
        source={source}
        layout={layout}
        onLayout={onLayout}
        perform={perform}
        pending={pending}
      />
      {source.id === "codex" && (
        <CodexModelSettings
          snapshot={snapshot}
          action={action}
          active={active}
          layout={layout}
          onLayout={onLayout}
        />
      )}
      <AgentRelays t={t} snapshot={snapshot} source={source} />
      <AgentWebSignIn
        t={t}
        source={source}
        action={action}
        perform={perform}
        pending={pending}
      />
      <Connection
        t={t}
        source={source}
        action={action}
        perform={perform}
        pending={pending}
      />
    </div>
  );
}
