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
import { ToggleRow } from "../SettingsControls";
import { Section } from "../SettingsPrimitives";
import AccountHomes from "../AccountHomes";
import AntigravityAccounts from "../AntigravityAccounts";
import { CodexModelSettings } from "../CodexModelSettings";
import { PanelBlocks } from "./PanelBlocks";
import { AgentQuotaReading } from "./AgentQuotaReading";
import { AgentRelays } from "./AgentRelays";
import { AgentWebSignIn } from "./AgentWebSignIn";
import { Connection } from "./Connection";
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
        <AccountHomes
          t={t}
          label={t("More Codex accounts")}
          variable="CODEX_HOME"
          placeholder="~/.codex-work"
          homes={snapshot.settings.codexHomes ?? []}
          onSave={(homes) =>
            action("set_setting", { key: "codexHomes", value: homes })
          }
        />
      )}
      {source.id === "claude" && (
        <AccountHomes
          t={t}
          label={t("More Claude Code accounts")}
          variable="CLAUDE_CONFIG_DIR"
          placeholder="~/.claude-work"
          homes={snapshot.settings.claudeHomes ?? []}
          onSave={(homes) =>
            action("set_setting", { key: "claudeHomes", value: homes })
          }
        />
      )}
      {source.id === "antigravity" && (
        <AntigravityAccounts
          t={t}
          settings={snapshot.settings.antigravityCliSettings ?? []}
          action={action}
        />
      )}
      {source.id === "copilot" && (
        <Section>
          <ToggleRow
            title={t("Read GitHub Copilot quota")}
            subtitle={t("Uses the GitHub sign-in on this computer.")}
            checked={snapshot.settings.readCopilotQuota === true}
            disabled={pending}
            change={(value) =>
              void perform(() =>
                action("set_setting", { key: "readCopilotQuota", value }),
              )
            }
          />
        </Section>
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
