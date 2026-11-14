/* One agent's settings: its switches, where its accounts come from, what the
 * panel shows of it, the relays it uses and its API credentials. */
import { useState } from "react";
import { RefreshCw } from "lucide-react";
import type {
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
import AccountHomes from "../AccountHomes";
import AntigravityAccounts from "../AntigravityAccounts";
import { CodexModelSettings } from "../CodexModelSettings";
import { PanelBlocks } from "./PanelBlocks";
import { AgentRelays } from "./AgentRelays";
import { AgentWebSignIn } from "./AgentWebSignIn";
import { Connection } from "./Connection";

export interface AgentDetailProps {
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
  preferences: Record<string, SourcePreferences>;
  onPreferences: (source: string, patch: SourcePreferences) => Promise<unknown>;
}

export function AgentDetail({
  t,
  snapshot,
  source,
  active,
  action,
  layout,
  onLayout,
  preferences,
  onPreferences,
}: AgentDetailProps) {
  // One save at a time. Its failure shows at the top of the detail, or through
  // `report` where the caller shows it beside its own buttons (Connection,
  // at the bottom of the page, where the top is out of view).
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function perform(
    job: () => Promise<unknown>,
    report: (message: string) => void = setError,
  ) {
    setPending(true);
    setError(null);
    try {
      await job();
      return true;
    } catch (error) {
      report(String(error));
      return false;
    } finally {
      setPending(false);
    }
  }

  const enabled = source.enabled;
  // What the client needs before it runs the hooks Agent HUD put in, such as
  // TRAE's own hooks switch.
  const hookStep =
    snapshot.connection.clientHooks?.clients?.find(
      (client) => client.source === source.id && client.installed,
    )?.notice ?? null;
  // The clients whose approval hook goes in only once the user turns it on.
  const optInApprovals = snapshot.settings.optInApprovals ?? [];
  const live = preferences[source.id]?.live !== false;
  const reminders = preferences[source.id]?.reminders !== false;
  const offReason = t("Turn on Enabled first.");
  const refreshLabel = t("Read all agents again");

  return (
    <Page>
      <header className="agents-detail-header">
        <HUDLogo source={source.id} size={24} />
        <div className="agents-detail-title">
          <h2>{source.name}</h2>
          {!source.detected && <p>{t("Not on this computer")}</p>}
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
      <Section footer={source.notice ?? hookStep ?? undefined}>
        <ToggleRow
          title={t("Enabled")}
          checked={enabled}
          disabled={pending}
          change={(value) =>
            void perform(() =>
              action("set_provider", { id: source.id, enabled: value }),
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
            void perform(() => onPreferences(source.id, { live: value }))
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
            void perform(() => onPreferences(source.id, { reminders: value }))
          }
        />
      </Section>
      <div
        className={`agents-detail-rest${enabled ? "" : " is-off"}`}
        inert={!enabled}
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
              disabled={pending}
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
    </Page>
  );
}
